//! Backdrop blur with the dual Kawase filter.
//!
//! Each region is copied out of the framebuffer (with a margin, so the edge
//! of the region still sees what lies around it), halved a few times with a
//! five-tap filter, doubled back with an eight-tap filter, and the last pass
//! is drawn straight into the framebuffer, clipped to the region's rounded
//! rectangle. All levels live in screen-sized textures; each region uses
//! their lower-left corner.

use glow::HasContext as _;

use crate::Blur;

/// The most halving passes; four already blur over 16× the pixel offset.
const MAX_PASSES: u32 = 4;

/// Pass count and sample offset for a [`Blur::strength`] (1–10).
pub(crate) fn passes(strength: u8) -> (u32, f32) {
    let strength = strength.clamp(1, 10);
    let passes = match strength {
        1..=2 => 1,
        3..=4 => 2,
        5..=7 => 3,
        _ => MAX_PASSES,
    };
    (passes, 1.0 + f32::from(strength) * 0.25)
}

/// A rectangle in framebuffer pixels, origin at the bottom left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GlRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// The region to blur and the area to sample for it, in framebuffer
/// coordinates, or `None` when the region is off screen.
pub(crate) fn regions(blur: &Blur, screen: [u32; 2]) -> Option<(GlRect, GlRect)> {
    let (sw, sh) = (screen[0] as i32, screen[1] as i32);
    let g = blur.area;
    let (x0, x1) = (
        g.loc.x.max(0),
        g.loc.x.saturating_add(g.size.w).min(sw),
    );
    let (top, bottom) = (
        g.loc.y.max(0),
        g.loc.y.saturating_add(g.size.h).min(sh),
    );
    if x1 <= x0 || bottom <= top {
        return None;
    }
    // egui and the framebuffer disagree on which way y grows.
    let region = GlRect {
        x: x0,
        y: sh - bottom,
        w: x1 - x0,
        h: bottom - top,
    };
    let (passes, offset) = passes(blur.strength);
    let margin = (offset * (1 << passes) as f32).ceil() as i32 * 2;
    let sx0 = (region.x - margin).max(0);
    let sy0 = (region.y - margin).max(0);
    let sx1 = (region.x + region.w + margin).min(sw);
    let sy1 = (region.y + region.h + margin).min(sh);
    let sample = GlRect {
        x: sx0,
        y: sy0,
        w: sx1 - sx0,
        h: sy1 - sy0,
    };
    Some((region, sample))
}

const VERTEX: &str = "#version 100
attribute vec2 a_pos;
varying vec2 v_uv;
uniform vec2 u_scale;
void main() {
    v_uv = (a_pos * 0.5 + 0.5) * u_scale;
    gl_Position = vec4(a_pos, 0.0, 1.0);
}";

const DOWN: &str = "#version 100
precision mediump float;
varying vec2 v_uv;
uniform sampler2D u_tex;
uniform vec2 u_half;
uniform vec2 u_max;
uniform float u_offset;
vec4 at(vec2 uv) { return texture2D(u_tex, clamp(uv, u_half, u_max)); }
void main() {
    vec2 o = u_half * u_offset;
    vec4 sum = at(v_uv) * 4.0;
    sum += at(v_uv - o);
    sum += at(v_uv + o);
    sum += at(v_uv + vec2(o.x, -o.y));
    sum += at(v_uv - vec2(o.x, -o.y));
    gl_FragColor = sum / 8.0;
}";

const UP: &str = "#version 100
precision mediump float;
varying vec2 v_uv;
uniform sampler2D u_tex;
uniform vec2 u_half;
uniform vec2 u_max;
uniform float u_offset;
uniform vec4 u_rect;
uniform float u_radius;
uniform float u_clip;
vec4 at(vec2 uv) { return texture2D(u_tex, clamp(uv, u_half, u_max)); }
void main() {
    vec2 o = u_half * u_offset;
    vec4 sum = at(v_uv + vec2(-o.x * 2.0, 0.0));
    sum += at(v_uv + vec2(-o.x, o.y)) * 2.0;
    sum += at(v_uv + vec2(0.0, o.y * 2.0));
    sum += at(v_uv + vec2(o.x, o.y)) * 2.0;
    sum += at(v_uv + vec2(o.x * 2.0, 0.0));
    sum += at(v_uv + vec2(o.x, -o.y)) * 2.0;
    sum += at(v_uv + vec2(0.0, -o.y * 2.0));
    sum += at(v_uv + vec2(-o.x, -o.y)) * 2.0;
    vec4 color = vec4((sum / 12.0).rgb, 1.0);
    float alpha = 1.0;
    if (u_clip > 0.5) {
        vec2 center = u_rect.xy + u_rect.zw * 0.5;
        vec2 q = abs(gl_FragCoord.xy - center) - (u_rect.zw * 0.5 - vec2(u_radius));
        float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - u_radius;
        alpha = clamp(0.5 - d, 0.0, 1.0);
    }
    gl_FragColor = color * alpha;
}";

struct Program {
    program: glow::Program,
    scale: Option<glow::UniformLocation>,
    half: Option<glow::UniformLocation>,
    max: Option<glow::UniformLocation>,
    offset: Option<glow::UniformLocation>,
    rect: Option<glow::UniformLocation>,
    radius: Option<glow::UniformLocation>,
    clip: Option<glow::UniformLocation>,
}

impl Program {
    /// # Safety
    ///
    /// A GL context must be current.
    unsafe fn new(gl: &glow::Context, fragment: &str) -> Result<Self, String> {
        // SAFETY: the caller guarantees a current context.
        unsafe {
            let program = gl.create_program()?;
            let mut shaders = Vec::new();
            for (kind, source) in [
                (glow::VERTEX_SHADER, VERTEX),
                (glow::FRAGMENT_SHADER, fragment),
            ] {
                let shader = gl.create_shader(kind)?;
                gl.shader_source(shader, source);
                gl.compile_shader(shader);
                if !gl.get_shader_compile_status(shader) {
                    let log = gl.get_shader_info_log(shader);
                    gl.delete_shader(shader);
                    gl.delete_program(program);
                    return Err(format!("blur shader: {log}"));
                }
                gl.attach_shader(program, shader);
                shaders.push(shader);
            }
            gl.bind_attrib_location(program, 0, "a_pos");
            gl.link_program(program);
            for shader in shaders {
                gl.detach_shader(program, shader);
                gl.delete_shader(shader);
            }
            if !gl.get_program_link_status(program) {
                let log = gl.get_program_info_log(program);
                gl.delete_program(program);
                return Err(format!("blur program: {log}"));
            }
            let at = |name| gl.get_uniform_location(program, name);
            Ok(Self {
                scale: at("u_scale"),
                half: at("u_half"),
                max: at("u_max"),
                offset: at("u_offset"),
                rect: at("u_rect"),
                radius: at("u_radius"),
                clip: at("u_clip"),
                program,
            })
        }
    }
}

/// GL objects for the filter; textures follow the screen size.
pub(crate) struct Blurrer {
    down: Program,
    up: Program,
    quad: glow::Buffer,
    fbo: glow::Framebuffer,
    levels: Vec<glow::Texture>,
    size: [u32; 2],
}

impl Blurrer {
    /// # Safety
    ///
    /// A GL context must be current.
    pub(crate) unsafe fn new(gl: &glow::Context) -> Result<Self, String> {
        // SAFETY: the caller guarantees a current context.
        unsafe {
            let down = Program::new(gl, DOWN)?;
            let up = Program::new(gl, UP)?;
            let quad = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(quad));
            let corners: [f32; 8] = [-1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0];
            let bytes: Vec<u8> = corners.iter().flat_map(|f| f.to_ne_bytes()).collect();
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &bytes, glow::STATIC_DRAW);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            let fbo = gl.create_framebuffer()?;
            Ok(Self {
                down,
                up,
                quad,
                fbo,
                levels: Vec::new(),
                size: [0, 0],
            })
        }
    }

    /// # Safety
    ///
    /// A GL context must be current.
    unsafe fn ensure_levels(&mut self, gl: &glow::Context, screen: [u32; 2]) -> Result<(), String> {
        if self.size == screen && !self.levels.is_empty() {
            return Ok(());
        }
        // SAFETY: the caller guarantees a current context.
        unsafe {
            for texture in self.levels.drain(..) {
                gl.delete_texture(texture);
            }
            for level in 0..=MAX_PASSES {
                let texture = gl.create_texture()?;
                gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                for (param, value) in [
                    (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
                    (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                    (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                    (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
                ] {
                    gl.tex_parameter_i32(glow::TEXTURE_2D, param, value as i32);
                }
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGB as i32,
                    (screen[0] >> level).max(1) as i32,
                    (screen[1] >> level).max(1) as i32,
                    0,
                    glow::RGB,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
                self.levels.push(texture);
            }
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
        self.size = screen;
        Ok(())
    }

    /// Blurs the framebuffer under each region, in order.
    ///
    /// # Safety
    ///
    /// A GL context must be current with the frame's framebuffer bound.
    pub(crate) unsafe fn apply(
        &mut self,
        gl: &glow::Context,
        screen: [u32; 2],
        blurs: &[Blur],
    ) -> Result<(), String> {
        if blurs.is_empty() || screen[0] == 0 || screen[1] == 0 {
            return Ok(());
        }
        // SAFETY: the caller guarantees a current context; every object used
        // below was created on it.
        unsafe {
            self.ensure_levels(gl, screen)?;
            let target = gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING);
            gl.disable(glow::BLEND);
            gl.disable(glow::SCISSOR_TEST);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.quad));
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 0, 0);
            for blur in blurs {
                if blur.strength == 0 {
                    continue;
                }
                if let Some((region, sample)) = regions(blur, screen) {
                    self.blur_one(gl, target, screen, blur, region, sample);
                }
            }
            gl.disable_vertex_attrib_array(0);
            gl.bind_framebuffer(glow::FRAMEBUFFER, target);
        }
        Ok(())
    }

    /// # Safety
    ///
    /// As for [`Blurrer::apply`], with the quad bound to attribute 0.
    unsafe fn blur_one(
        &self,
        gl: &glow::Context,
        target: Option<glow::Framebuffer>,
        screen: [u32; 2],
        blur: &Blur,
        region: GlRect,
        sample: GlRect,
    ) {
        let (passes, offset) = passes(blur.strength);
        let level_size = |level: u32| [(screen[0] >> level).max(1), (screen[1] >> level).max(1)];
        let used = |level: u32| {
            [
                (sample.w >> level).max(1) as f32,
                (sample.h >> level).max(1) as f32,
            ]
        };
        // SAFETY: forwarded from the caller.
        unsafe {
            // Copy the backdrop into the corner of level 0.
            gl.bind_framebuffer(glow::FRAMEBUFFER, target);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.levels[0]));
            gl.copy_tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                0,
                0,
                sample.x,
                sample.y,
                sample.w,
                sample.h,
            );

            let set = |p: &Program, from: u32| {
                let size = level_size(from);
                let used = used(from);
                let (sx, sy) = (used[0] / size[0] as f32, used[1] / size[1] as f32);
                let half = [0.5 / size[0] as f32, 0.5 / size[1] as f32];
                gl.uniform_2_f32(p.scale.as_ref(), sx, sy);
                gl.uniform_2_f32(p.half.as_ref(), half[0], half[1]);
                gl.uniform_2_f32(p.max.as_ref(), sx - half[0], sy - half[1]);
                gl.uniform_1_f32(p.offset.as_ref(), offset);
            };

            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            gl.use_program(Some(self.down.program));
            for level in 0..passes {
                let to = used(level + 1);
                gl.framebuffer_texture_2d(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::TEXTURE_2D,
                    Some(self.levels[level as usize + 1]),
                    0,
                );
                gl.viewport(0, 0, to[0] as i32, to[1] as i32);
                gl.bind_texture(glow::TEXTURE_2D, Some(self.levels[level as usize]));
                set(&self.down, level);
                gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            }

            gl.use_program(Some(self.up.program));
            gl.uniform_1_f32(self.up.clip.as_ref(), 0.0);
            for level in (2..=passes).rev() {
                let to = used(level - 1);
                gl.framebuffer_texture_2d(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::TEXTURE_2D,
                    Some(self.levels[level as usize - 1]),
                    0,
                );
                gl.viewport(0, 0, to[0] as i32, to[1] as i32);
                gl.bind_texture(glow::TEXTURE_2D, Some(self.levels[level as usize]));
                set(&self.up, level);
                gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            }
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                None,
                0,
            );

            // The last doubling lands in the framebuffer, clipped to the
            // region's rounded rectangle.
            gl.bind_framebuffer(glow::FRAMEBUFFER, target);
            gl.viewport(sample.x, sample.y, sample.w, sample.h);
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(region.x, region.y, region.w, region.h);
            gl.enable(glow::BLEND);
            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.levels[1]));
            set(&self.up, 1);
            let radius = f32::from(blur.corner_radius)
                .min(blur.area.size.w.max(0) as f32 / 2.0)
                .min(blur.area.size.h.max(0) as f32 / 2.0);
            gl.uniform_4_f32(
                self.up.rect.as_ref(),
                blur.area.loc.x as f32,
                screen[1] as f32 - (blur.area.loc.y as f32 + blur.area.size.h as f32),
                blur.area.size.w as f32,
                blur.area.size.h as f32,
            );
            gl.uniform_1_f32(self.up.radius.as_ref(), radius);
            gl.uniform_1_f32(self.up.clip.as_ref(), 1.0);
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::BLEND);
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
        }
    }
}

#[cfg(test)]
mod tests {
    use mcsapi::Geometry;

    use super::*;

    fn blur(x: i32, y: i32, w: i32, h: i32, strength: u8) -> Blur {
        Blur {
            area: Geometry::new((x, y).into(), (w, h).into()),
            corner_radius: 0,
            strength,
        }
    }

    #[test]
    fn stronger_blurs_use_more_passes() {
        let mut last = (0, 0.0);
        for strength in 1..=10 {
            let p = passes(strength);
            assert!(p.0 >= last.0 && p.1 > last.1, "strength {strength}");
            assert!((1..=MAX_PASSES).contains(&p.0));
            last = p;
        }
        assert_eq!(passes(0), passes(1));
        assert_eq!(passes(200), passes(10));
    }

    #[test]
    fn regions_flip_y_and_add_a_clamped_margin() {
        let (region, sample) = regions(&blur(0, 0, 1280, 32, 6), [1280, 800]).unwrap();
        assert_eq!(
            region,
            GlRect {
                x: 0,
                y: 768,
                w: 1280,
                h: 32
            }
        );
        assert_eq!((sample.x, sample.w), (0, 1280));
        assert_eq!(sample.y + sample.h, 800);
        assert!(sample.y < region.y);
    }

    #[test]
    fn off_screen_regions_are_skipped_and_partial_ones_clipped() {
        assert_eq!(regions(&blur(2000, 0, 100, 100, 5), [1280, 800]), None);
        assert_eq!(regions(&blur(0, 0, 0, 100, 5), [1280, 800]), None);
        let (region, _) = regions(&blur(-50, 700, 100, 200, 5), [1280, 800]).unwrap();
        assert_eq!(
            region,
            GlRect {
                x: 0,
                y: 0,
                w: 50,
                h: 100
            }
        );
    }
}

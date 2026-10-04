//! 1:1 touchpad gestures: content follows the fingers exactly, then coasts.
//!
//! A [`GestureTracker`] turns swipes, pinches and rotations into a
//! [`Transform`] for the content under the fingers. While fingers are down the
//! mapping is exact: the point under the fingers stays under them, whatever
//! they do. When they lift with some speed, the content keeps moving and slows
//! down (momentum); resting fingers on the touchpad again catches it.
//!
//! Apps feed the tracker from egui with [`GestureTracker::update`]. Hosts with
//! raw touchpad events (libinput, `zwp_pointer_gestures_v1`) feed it with
//! [`GestureTracker::handle`], or translate those events for an egui app with
//! [`EguiBridge`].
//!
//! ```
//! use mcsapi_ui::egui;
//! use mcsapi_ui::gesture::{GestureEvent, GestureTracker};
//!
//! let mut tracker = GestureTracker::default();
//! let fingers = egui::pos2(100.0, 100.0);
//! tracker.handle(GestureEvent::PinchBegin { fingers: 2 }, fingers, 0.0);
//! tracker.handle(
//!     GestureEvent::PinchUpdate { delta: egui::Vec2::ZERO, scale: 2.0, rotation: 0.0 },
//!     fingers,
//!     0.016,
//! );
//! // The point under the fingers did not move; everything else doubled.
//! let t = tracker.transform();
//! assert_eq!(t.apply(fingers), fingers);
//! assert_eq!(t.apply(egui::pos2(110.0, 100.0)), egui::pos2(120.0, 100.0));
//! ```

use std::ops::RangeInclusive;

use egui::{Event, Modifiers, MouseWheelUnit, Pos2, Rect, TouchPhase, Vec2, emath::Rot2};

/// A touchpad gesture event, shaped like libinput's and
/// `zwp_pointer_gestures_v1`'s.
///
/// Distances are logical points, rotations radians clockwise. Every gesture
/// is a begin, any number of updates, and an end.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum GestureEvent {
    /// Fingers started moving together (two-finger scrolling or a
    /// multi-finger swipe).
    SwipeBegin {
        /// Fingers on the touchpad.
        fingers: u32,
    },
    /// The fingers' center moved.
    SwipeUpdate {
        /// Movement since the last event.
        delta: Vec2,
    },
    /// The fingers lifted.
    SwipeEnd {
        /// The gesture was interrupted (for example by another finger) rather
        /// than finished.
        cancelled: bool,
    },
    /// Fingers started pinching or rotating.
    PinchBegin {
        /// Fingers on the touchpad.
        fingers: u32,
    },
    /// The pinch changed.
    PinchUpdate {
        /// Movement of the fingers' center since the last event.
        delta: Vec2,
        /// Finger spread relative to the begin event (1 = unchanged), not to
        /// the previous update, so rounding never accumulates.
        scale: f32,
        /// Rotation since the last event, radians clockwise.
        rotation: f32,
    },
    /// The fingers lifted.
    PinchEnd {
        /// The gesture was interrupted rather than finished.
        cancelled: bool,
    },
    /// Fingers came to rest on the touchpad.
    HoldBegin {
        /// Fingers on the touchpad.
        fingers: u32,
    },
    /// Resting fingers lifted or started moving.
    HoldEnd {
        /// The fingers started a swipe or pinch rather than lifting.
        cancelled: bool,
    },
}

impl GestureEvent {
    /// Whether this starts a gesture.
    pub fn is_begin(&self) -> bool {
        matches!(
            self,
            Self::SwipeBegin { .. } | Self::PinchBegin { .. } | Self::HoldBegin { .. }
        )
    }

    /// Whether this ends a gesture.
    pub fn is_end(&self) -> bool {
        matches!(
            self,
            Self::SwipeEnd { .. } | Self::PinchEnd { .. } | Self::HoldEnd { .. }
        )
    }
}

/// Where content is drawn: scaled, then rotated about the origin, then
/// translated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    /// Offset in points.
    pub translation: Vec2,
    /// Uniform scale; 1 is unscaled.
    pub scale: f32,
    /// Rotation in radians, clockwise.
    pub rotation: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    /// Leaves content where it is.
    pub const IDENTITY: Self = Self {
        translation: Vec2::ZERO,
        scale: 1.0,
        rotation: 0.0,
    };

    /// Where a content point is drawn.
    pub fn apply(&self, content: Pos2) -> Pos2 {
        (self.rot2() * (content.to_vec2() * self.scale) + self.translation).to_pos2()
    }

    /// The content point drawn at `screen`; the inverse of [`Self::apply`].
    pub fn invert(&self, screen: Pos2) -> Pos2 {
        (self.rot2().inverse() * ((screen.to_vec2() - self.translation) / self.scale)).to_pos2()
    }

    /// The rotation as an [`Rot2`].
    pub fn rot2(&self) -> Rot2 {
        Rot2::from_angle(self.rotation)
    }

    /// Translation and scale for [`egui::Context::set_transform_layer`].
    /// egui layers cannot rotate, so this drops the rotation.
    pub fn layer(&self) -> egui::emath::TSTransform {
        egui::emath::TSTransform::new(self.translation, self.scale)
    }

    /// Moves by `motion`, scaling and rotating about `pivot` so the content
    /// under the pivot stays under it.
    fn moved(self, motion: Motion, pivot: Pos2, scale_range: &RangeInclusive<f32>) -> Self {
        // min/max rather than clamp: a reversed or NaN range must not panic.
        let wanted = self.scale * motion.log_scale.exp();
        let scale = wanted.max(*scale_range.start()).min(*scale_range.end());
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            self.scale
        };
        let k = scale / self.scale;
        let rotation = Rot2::from_angle(motion.rotation);
        let pivot = pivot.to_vec2();
        let translation = self.translation + motion.translation;
        Self {
            translation: pivot + rotation * ((translation - pivot) * k),
            scale,
            rotation: self.rotation + motion.rotation,
        }
    }
}

/// A change of a [`Transform`], or its rate of change per second.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Motion {
    translation: Vec2,
    /// Natural log of the scale factor, so it adds up and decays like the rest.
    log_scale: f32,
    rotation: f32,
}

impl Motion {
    fn add(self, other: Self) -> Self {
        Self {
            translation: self.translation + other.translation,
            log_scale: self.log_scale + other.log_scale,
            rotation: self.rotation + other.rotation,
        }
    }

    fn scaled(self, by: f32) -> Self {
        Self {
            translation: self.translation * by,
            log_scale: self.log_scale * by,
            rotation: self.rotation * by,
        }
    }

    fn lerp(self, to: Self, t: f32) -> Self {
        self.scaled(1.0 - t).add(to.scaled(t))
    }
}

/// How a [`GestureTracker`] coasts and limits content.
#[derive(Clone, Debug, PartialEq)]
pub struct GestureSettings {
    /// Keep moving after the fingers lift.
    pub momentum: bool,
    /// How fast panning slows down, per second; higher stops sooner.
    pub pan_friction: f32,
    /// How fast zooming and rotating slow down, per second.
    pub transform_friction: f32,
    /// Fingers resting longer than this before lifting cancel momentum, in
    /// seconds.
    pub release_window: f64,
    /// Panning slower than this (points per second) stops.
    pub min_pan_speed: f32,
    /// Fastest panning momentum, in points per second.
    pub max_pan_speed: f32,
    /// Allowed [`Transform::scale`] values. A reversed range pins the scale
    /// to its end; non-positive or NaN results leave the scale unchanged.
    pub scale_range: RangeInclusive<f32>,
    /// Apply pinch rotation; off for content that should only pan and zoom.
    pub rotate: bool,
}

impl Default for GestureSettings {
    fn default() -> Self {
        Self {
            momentum: true,
            pan_friction: 2.5,
            transform_friction: 8.0,
            release_window: 0.1,
            min_pan_speed: 15.0,
            max_pan_speed: 8000.0,
            scale_range: 0.05..=50.0,
            rotate: true,
        }
    }
}

/// What the fingers are doing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum GesturePhase {
    /// Nothing is moving.
    Idle,
    /// Fingers are on the touchpad and content follows them.
    Tracking {
        /// Fingers on the touchpad, when the source reports it.
        fingers: u32,
    },
    /// The fingers lifted and the content is slowing down.
    Coasting,
}

/// Motion recorded in one batch of events sharing a timestamp.
#[derive(Clone, Copy, Debug, Default)]
struct Batch {
    /// Time of the batch before this one.
    since: f64,
    time: f64,
    motion: Motion,
}

/// Follows touchpad gestures 1:1 and coasts after release.
///
/// Holds the content [`Transform`]; read it every frame to draw. See the
/// [module docs](self).
#[derive(Clone, Debug, Default)]
pub struct GestureTracker {
    settings: GestureSettings,
    transform: Transform,
    phase: Phase,
    pivot: Pos2,
    velocity: Motion,
    batch: Batch,
    last_update: f64,
    last_tick: f64,
    pinch_scale: f32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Phase {
    #[default]
    Idle,
    Tracking(u32),
    Coasting,
}

impl GestureTracker {
    /// A tracker at the identity transform.
    pub fn new(settings: GestureSettings) -> Self {
        Self {
            settings,
            ..Self::default()
        }
    }

    /// The current content transform.
    pub fn transform(&self) -> Transform {
        self.transform
    }

    /// Replaces the transform, for example to reset the view, and stops any
    /// momentum.
    pub fn set_transform(&mut self, transform: Transform) {
        self.transform = transform;
        if self.phase == Phase::Coasting {
            self.phase = Phase::Idle;
        }
    }

    /// The settings in use.
    pub fn settings(&self) -> &GestureSettings {
        &self.settings
    }

    /// Changes the settings; takes effect from the next event.
    pub fn settings_mut(&mut self) -> &mut GestureSettings {
        &mut self.settings
    }

    /// What the fingers are doing.
    pub fn phase(&self) -> GesturePhase {
        match self.phase {
            Phase::Idle => GesturePhase::Idle,
            Phase::Tracking(fingers) => GesturePhase::Tracking { fingers },
            Phase::Coasting => GesturePhase::Coasting,
        }
    }

    /// Whether the content is still moving without new events, so the host
    /// should keep calling [`Self::tick`] (or repainting).
    pub fn is_animating(&self) -> bool {
        self.phase == Phase::Coasting
    }

    /// Handles one gesture event at `time` seconds. `pivot` is where the
    /// fingers point (usually the pointer position), in the same coordinates
    /// as the transform's output.
    ///
    /// Updates without a begin (a plain mouse wheel, say) still move the
    /// content, but never start momentum.
    pub fn handle(&mut self, event: GestureEvent, pivot: Pos2, time: f64) {
        self.tick(time);
        self.pivot = pivot;
        match event {
            GestureEvent::SwipeBegin { fingers }
            | GestureEvent::PinchBegin { fingers }
            | GestureEvent::HoldBegin { fingers } => self.begin(fingers, time),
            GestureEvent::SwipeUpdate { delta } => self.advance(
                Motion {
                    translation: delta,
                    ..Motion::default()
                },
                time,
            ),
            GestureEvent::PinchUpdate {
                delta,
                scale,
                rotation,
            } => {
                let log_scale = if scale > 0.0 && scale.is_finite() && self.pinch_scale > 0.0 {
                    let log_scale = (scale / self.pinch_scale).ln();
                    self.pinch_scale = scale;
                    log_scale
                } else {
                    0.0
                };
                let rotation = if self.settings.rotate && rotation.is_finite() {
                    rotation
                } else {
                    0.0
                };
                self.advance(
                    Motion {
                        translation: delta,
                        log_scale,
                        rotation,
                    },
                    time,
                );
            }
            GestureEvent::SwipeEnd { cancelled } | GestureEvent::PinchEnd { cancelled } => {
                self.release(time, !cancelled);
            }
            GestureEvent::HoldEnd { .. } => self.release(time, false),
        }
    }

    /// Advances momentum to `time` seconds. Call every frame while
    /// [`Self::is_animating`]; earlier times are ignored.
    pub fn tick(&mut self, time: f64) {
        if self.phase != Phase::Coasting {
            return;
        }
        let dt = (time - self.last_tick) as f32;
        if dt <= 0.0 {
            return;
        }
        self.last_tick = time;
        // Exact exponential decay: distance covered is v * (1 - e^-λt) / λ.
        let decay = |friction: f32| {
            let friction = friction.max(f32::EPSILON);
            let remaining = (-friction * dt).exp();
            (remaining, (1.0 - remaining) / friction)
        };
        let (pan_left, pan_moved) = decay(self.settings.pan_friction);
        let (rest_left, rest_moved) = decay(self.settings.transform_friction);
        let v = self.velocity;
        let motion = Motion {
            translation: v.translation * pan_moved,
            log_scale: v.log_scale * rest_moved,
            rotation: v.rotation * rest_moved,
        };
        self.transform = self
            .transform
            .moved(motion, self.pivot, &self.settings.scale_range);
        self.velocity = Motion {
            translation: v.translation * pan_left,
            log_scale: v.log_scale * rest_left,
            rotation: v.rotation * rest_left,
        };
        if !self.moving() {
            self.phase = Phase::Idle;
            self.velocity = Motion::default();
        }
    }

    /// Feeds the tracker from this frame's egui input and keeps momentum
    /// going. Returns whether the transform changed.
    ///
    /// Gestures count only while the pointer is over `area` (or once they
    /// started there). Hosts deliver touchpad gestures as wheel events with
    /// [`TouchPhase::Start`] and [`TouchPhase::End`] around them, plus
    /// [`Event::Zoom`] and [`Event::Rotate`]; [`EguiBridge`] does this.
    /// Wheels without phases scroll without momentum, and Ctrl+wheel zooms.
    pub fn update(&mut self, ui: &egui::Ui, area: Rect) -> bool {
        let before = self.transform;
        let (events, time, hover) = ui.input(|i| (i.events.clone(), i.time, i.pointer.hover_pos()));
        self.tick(time);
        let tracking = matches!(self.phase, Phase::Tracking(_));
        if tracking || hover.is_some_and(|p| area.contains(p)) {
            let pivot = hover.unwrap_or_else(|| area.center());
            for event in &events {
                self.handle_egui(event, pivot, time, area);
            }
        }
        // While fingers are down, their next event triggers the repaint.
        if self.is_animating() {
            ui.ctx().request_repaint();
        }
        self.transform != before
    }

    fn handle_egui(&mut self, event: &Event, pivot: Pos2, time: f64, area: Rect) {
        match *event {
            Event::MouseWheel {
                unit,
                delta,
                phase,
                modifiers,
            } => {
                let delta = match unit {
                    MouseWheelUnit::Point => delta,
                    MouseWheelUnit::Line => delta * 40.0,
                    MouseWheelUnit::Page => delta * area.height(),
                };
                if modifiers.command || modifiers.ctrl {
                    let factor = (delta.y / 200.0).exp();
                    self.handle_pinch(factor, 0.0, pivot, time);
                    return;
                }
                match phase {
                    TouchPhase::Start => {
                        self.handle(GestureEvent::SwipeBegin { fingers: 2 }, pivot, time);
                    }
                    TouchPhase::Move => {}
                    TouchPhase::End | TouchPhase::Cancel => {
                        if delta != Vec2::ZERO {
                            self.handle(GestureEvent::SwipeUpdate { delta }, pivot, time);
                        }
                        let cancelled = phase == TouchPhase::Cancel;
                        self.handle(GestureEvent::SwipeEnd { cancelled }, pivot, time);
                        return;
                    }
                }
                if delta != Vec2::ZERO {
                    self.handle(GestureEvent::SwipeUpdate { delta }, pivot, time);
                }
            }
            Event::Zoom(factor) => self.handle_pinch(factor, 0.0, pivot, time),
            Event::Rotate(rotation) => self.handle_pinch(1.0, rotation, pivot, time),
            _ => {}
        }
    }

    /// A pinch step from egui, which reports `factor` relative to the last
    /// step rather than to the begin.
    fn handle_pinch(&mut self, factor: f32, rotation: f32, pivot: Pos2, time: f64) {
        // Outside a gesture (Ctrl+wheel, or a host without phases) every
        // event is its own pinch.
        if !matches!(self.phase, Phase::Tracking(_)) || self.pinch_scale <= 0.0 {
            self.pinch_scale = 1.0;
        }
        let scale = self.pinch_scale * factor;
        self.handle(
            GestureEvent::PinchUpdate {
                delta: Vec2::ZERO,
                scale,
                rotation,
            },
            pivot,
            time,
        );
    }

    fn begin(&mut self, fingers: u32, time: f64) {
        self.phase = Phase::Tracking(fingers);
        self.velocity = Motion::default();
        self.batch = Batch {
            since: time,
            time,
            motion: Motion::default(),
        };
        self.last_update = time;
        self.pinch_scale = 1.0;
    }

    fn advance(&mut self, motion: Motion, time: f64) {
        if self.pinch_scale <= 0.0 {
            self.pinch_scale = 1.0;
        }
        self.transform = self
            .transform
            .moved(motion, self.pivot, &self.settings.scale_range);
        match self.phase {
            Phase::Tracking(_) => {
                if time > self.batch.time {
                    self.flush();
                    self.batch.since = self.batch.time;
                    self.batch.time = time;
                }
                self.batch.motion = self.batch.motion.add(motion);
                self.last_update = time;
            }
            // Fingers moving without a begin stop any coasting.
            Phase::Coasting => self.phase = Phase::Idle,
            Phase::Idle => {}
        }
    }

    /// Folds the current batch into the velocity estimate.
    fn flush(&mut self) {
        let dt = (self.batch.time - self.batch.since) as f32;
        if dt > 0.0 {
            let sample = self.batch.motion.scaled(1.0 / dt);
            self.velocity = if self.velocity == Motion::default() {
                sample
            } else {
                self.velocity.lerp(sample, 0.6)
            };
        }
        self.batch.motion = Motion::default();
    }

    fn release(&mut self, time: f64, coast: bool) {
        if !matches!(self.phase, Phase::Tracking(_)) {
            return;
        }
        self.flush();
        let fresh = time - self.last_update <= self.settings.release_window;
        self.pinch_scale = 1.0;
        let speed = self.velocity.translation.length();
        if speed > self.settings.max_pan_speed {
            self.velocity.translation *= self.settings.max_pan_speed / speed;
        }
        if coast && fresh && self.settings.momentum && self.moving() {
            self.phase = Phase::Coasting;
            self.last_tick = time;
        } else {
            self.phase = Phase::Idle;
            self.velocity = Motion::default();
        }
    }

    fn moving(&self) -> bool {
        self.velocity.translation.length() >= self.settings.min_pan_speed
            || self.velocity.log_scale.abs() >= 0.02
            || self.velocity.rotation.abs() >= 0.02
    }
}

/// Translates raw gesture events into egui events for an app drawn with egui,
/// the way [`GestureTracker::update`] reads them.
///
/// Swipes and pinch movement become wheel events (in points, phase `Start`
/// at the begin and `End`/`Cancel` at the end), pinch spread becomes
/// [`Event::Zoom`] and rotation [`Event::Rotate`]. Resting fingers become an
/// empty, cancelled wheel gesture, which stops momentum. Finger counts do not
/// survive, so hosts should keep swipes of three or more fingers for
/// themselves.
#[derive(Clone, Debug, Default)]
pub struct EguiBridge {
    pinch_scale: f32,
}

impl EguiBridge {
    /// The egui events for one gesture event.
    pub fn events(&mut self, event: GestureEvent, modifiers: Modifiers) -> Vec<Event> {
        let wheel = |delta: Vec2, phase| Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta,
            phase,
            modifiers,
        };
        let end = |cancelled: bool| {
            wheel(
                Vec2::ZERO,
                if cancelled {
                    TouchPhase::Cancel
                } else {
                    TouchPhase::End
                },
            )
        };
        match event {
            GestureEvent::SwipeBegin { .. }
            | GestureEvent::PinchBegin { .. }
            | GestureEvent::HoldBegin { .. } => {
                self.pinch_scale = 1.0;
                vec![wheel(Vec2::ZERO, TouchPhase::Start)]
            }
            GestureEvent::SwipeUpdate { delta } => vec![wheel(delta, TouchPhase::Move)],
            GestureEvent::PinchUpdate {
                delta,
                scale,
                rotation,
            } => {
                let mut events = Vec::with_capacity(3);
                if delta != Vec2::ZERO {
                    events.push(wheel(delta, TouchPhase::Move));
                }
                if scale > 0.0 && scale.is_finite() && self.pinch_scale > 0.0 {
                    let factor = scale / self.pinch_scale;
                    self.pinch_scale = scale;
                    if factor != 1.0 {
                        events.push(Event::Zoom(factor));
                    }
                }
                if rotation != 0.0 {
                    events.push(Event::Rotate(rotation));
                }
                events
            }
            GestureEvent::SwipeEnd { cancelled } | GestureEvent::PinchEnd { cancelled } => {
                vec![end(cancelled)]
            }
            GestureEvent::HoldEnd { .. } => vec![end(true)],
        }
    }
}

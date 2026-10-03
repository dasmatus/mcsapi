use crate::{Error, Geometry, WindowId};

/// A window and its proposed logical geometry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Placement {
    /// The window to configure.
    pub window: WindowId,
    /// Bounds to apply through the host's Smithay configure/rendering path.
    pub geometry: Geometry,
}

/// Simple xmonad-like tiling policies.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum Layout {
    /// One main window on the left, with the rest stacked vertically on the right.
    ///
    /// The main pane receives half the width, rounding down. A single window
    /// fills the output. Remainder pixels are distributed from the top.
    #[default]
    Tall,
    /// Every window fills the output; the host displays only the focused one.
    Monocle,
}

impl Layout {
    /// Writes placements without allocating once the buffer has enough capacity.
    ///
    /// Invalid bounds or sub-pixel tiles return an error without modifying the
    /// buffer. Coordinates and edges must fit in `i32`, including negative origins.
    pub fn arrange(
        self,
        bounds: Geometry,
        windows: &[WindowId],
        placements: &mut Vec<Placement>,
    ) -> Result<(), Error> {
        let (width, height) = (bounds.size.w, bounds.size.h);
        if width <= 0
            || height <= 0
            || bounds.loc.x.checked_add(width).is_none()
            || bounds.loc.y.checked_add(height).is_none()
        {
            return Err(Error::InvalidGeometry);
        }
        let stack_count = windows.len().saturating_sub(1);
        if self == Self::Tall
            && stack_count > 0
            && (width < 2 || stack_count > height as usize)
        {
            return Err(Error::InsufficientSpace);
        }
        placements.clear();
        placements.reserve(windows.len());
        if self == Self::Monocle || windows.len() <= 1 {
            placements.extend(windows.iter().map(|&window| Placement {
                window,
                geometry: bounds,
            }));
            return Ok(());
        }

        let main_width = width / 2;
        placements.push(Placement {
            window: windows[0],
            geometry: Geometry::from_loc_and_size(bounds.loc, (main_width, height)),
        });
        let count = stack_count as i32;
        let tile_height = height / count;
        let remainder = height % count;
        let mut y = bounds.loc.y;
        for (index, &window) in windows[1..].iter().enumerate() {
            let h = tile_height + i32::from((index as i32) < remainder);
            placements.push(Placement {
                window,
                geometry: Geometry::from_loc_and_size(
                    (bounds.loc.x + main_width, y),
                    (width - main_width, h),
                ),
            });
            y += h;
        }
        Ok(())
    }
}

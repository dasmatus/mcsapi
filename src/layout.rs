use std::iter::FusedIterator;

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
    /// Returns lazy placements with no intermediate collection or allocation.
    ///
    /// Bounds must be positive, edges must fit in `i32`, and every tile must
    /// receive at least one logical pixel. Validation precedes iteration.
    pub fn arrange<I>(self, bounds: Geometry, windows: I) -> Result<Placements<I::IntoIter>, Error>
    where
        I: IntoIterator<Item = WindowId>,
        I::IntoIter: ExactSizeIterator,
    {
        let windows = windows.into_iter();
        let (width, height) = (bounds.size.w, bounds.size.h);
        if width <= 0
            || height <= 0
            || bounds.loc.x.checked_add(width).is_none()
            || bounds.loc.y.checked_add(height).is_none()
        {
            return Err(Error::InvalidGeometry);
        }
        let count = windows.len();
        if self == Self::Tall && count > 1 && (width < 2 || count - 1 > height as usize) {
            return Err(Error::InsufficientSpace);
        }
        Ok(Placements {
            windows,
            layout: self,
            bounds,
            count,
            index: 0,
        })
    }
}

/// An allocation-free iterator over validated logical window placements.
#[derive(Clone, Debug)]
pub struct Placements<I> {
    windows: I,
    layout: Layout,
    bounds: Geometry,
    count: usize,
    index: usize,
}

impl<I: ExactSizeIterator<Item = WindowId>> Iterator for Placements<I> {
    type Item = Placement;

    fn next(&mut self) -> Option<Self::Item> {
        let window = self.windows.next()?;
        let geometry = if self.layout == Layout::Monocle || self.count <= 1 {
            self.bounds
        } else {
            let main_width = self.bounds.size.w / 2;
            if self.index == 0 {
                Geometry::new(self.bounds.loc, (main_width, self.bounds.size.h).into())
            } else {
                let count = (self.count - 1) as i32;
                let index = (self.index - 1) as i32;
                let height = self.bounds.size.h / count;
                let remainder = self.bounds.size.h % count;
                Geometry::new(
                    (
                        self.bounds.loc.x + main_width,
                        self.bounds.loc.y + index * height + index.min(remainder),
                    )
                        .into(),
                    (
                        self.bounds.size.w - main_width,
                        height + i32::from(index < remainder),
                    )
                        .into(),
                )
            }
        };
        self.index += 1;
        Some(Placement { window, geometry })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.windows.size_hint()
    }
}

impl<I: ExactSizeIterator<Item = WindowId>> ExactSizeIterator for Placements<I> {}
impl<I: ExactSizeIterator<Item = WindowId> + FusedIterator> FusedIterator for Placements<I> {}

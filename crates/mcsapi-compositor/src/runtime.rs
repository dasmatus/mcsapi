//! Runtime clients: programs the compositor starts on a private connection.
//!
//! GPUI draws only into windows it opens itself, as an ordinary Wayland
//! client, and does not speak wlr-layer-shell. A desktop that wants its panel
//! or launcher drawn with GPUI still needs those windows placed like a panel
//! or an overlay, and kept out of the tiling. So the compositor starts such a
//! program itself: it creates a socket pair, keeps one end as a Wayland
//! client, and hands the other to the child as `WAYLAND_SOCKET`, which
//! libwayland (and so GPUI) connects to before `WAYLAND_DISPLAY`. Every
//! toplevel arriving on that connection gets the [`Role`] it was started
//! with. Nothing a client says can give it a role: an ordinary client on the
//! public socket is always an app window.
//!
//! The child also gets `MCSAPI_ROLE` (`app`, `panel` or `overlay`), so one
//! program can serve several roles and a panel can draw no title bar.

use std::time::{Duration, Instant};

use mcsapi::Geometry;

/// An edge of the output.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Edge {
    /// Top.
    Top,
    /// Bottom.
    Bottom,
    /// Left.
    Left,
    /// Right.
    Right,
}

/// How a runtime client's windows sit on the output.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Role {
    /// An ordinary window: given to the shell, tiled, decorated and focused
    /// like any Wayland client.
    App,
    /// A bar along `edge`, the full length of the output and `size` logical
    /// pixels thick, above windows. Its strip is reserved
    /// ([`crate::Shell::set_reserved`]). With `keyboard`, clicking it gives it
    /// the keyboard until a click elsewhere, for search fields; without, it
    /// never has keyboard focus.
    Panel {
        /// The edge it is attached to.
        edge: Edge,
        /// Thickness in logical pixels.
        size: i32,
        /// Whether a click gives it the keyboard.
        keyboard: bool,
    },
    /// The whole output, above everything including the shell's chrome, with
    /// every key and pointer event while it is mapped: launchers, palettes,
    /// on-screen displays. It should draw a translucent background itself.
    Overlay,
}

impl Role {
    /// The value of `MCSAPI_ROLE` given to the client.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Panel { .. } => "panel",
            Self::Overlay => "overlay",
        }
    }

    /// Where a window with this role goes on an output of `size`; `None` for
    /// [`Role::App`], which the shell places.
    pub fn geometry(self, size: (i32, i32)) -> Option<Geometry> {
        let (w, h) = size;
        let rect = |x, y, w: i32, h: i32| Geometry::new((x, y).into(), (w.max(1), h.max(1)).into());
        match self {
            Self::App => None,
            Self::Overlay => Some(rect(0, 0, w, h)),
            Self::Panel { edge, size, .. } => {
                let size = size.clamp(
                    1,
                    match edge {
                        Edge::Top | Edge::Bottom => h.max(1),
                        Edge::Left | Edge::Right => w.max(1),
                    },
                );
                Some(match edge {
                    Edge::Top => rect(0, 0, w, size),
                    Edge::Bottom => rect(0, h - size, w, size),
                    Edge::Left => rect(0, 0, size, h),
                    Edge::Right => rect(w - size, 0, size, h),
                })
            }
        }
    }
}

/// Strips of the output covered by runtime panels, in logical pixels.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Reserved {
    /// Along the top edge.
    pub top: i32,
    /// Along the bottom edge.
    pub bottom: i32,
    /// Along the left edge.
    pub left: i32,
    /// Along the right edge.
    pub right: i32,
}

impl Reserved {
    /// The strips mapped panels with these roles cover. Panels on one edge
    /// stack, so their sizes add up.
    pub fn of(roles: impl IntoIterator<Item = Role>) -> Self {
        let mut reserved = Self::default();
        for role in roles {
            if let Role::Panel { edge, size, .. } = role {
                let size = size.max(0);
                match edge {
                    Edge::Top => reserved.top += size,
                    Edge::Bottom => reserved.bottom += size,
                    Edge::Left => reserved.left += size,
                    Edge::Right => reserved.right += size,
                }
            }
        }
        reserved
    }

    /// `bounds` without the reserved strips.
    pub fn shrink(self, bounds: Geometry) -> Geometry {
        let mut g = bounds;
        g.loc.x += self.left;
        g.loc.y += self.top;
        g.size.w = (g.size.w - self.left - self.right).max(1);
        g.size.h = (g.size.h - self.top - self.bottom).max(1);
        g
    }
}

/// A program the compositor starts with a [`Role`].
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RuntimeClient {
    /// Program and arguments.
    pub argv: Vec<String>,
    /// What its windows are.
    pub role: Role,
    /// Start it again when it exits, as a panel should. A client that exits
    /// within [`RuntimeClient::MIN_UPTIME`] of starting is not restarted, so
    /// one that crashes on start does not loop.
    pub restart: bool,
}

impl RuntimeClient {
    /// How long a client must have run to be restarted.
    pub const MIN_UPTIME: Duration = Duration::from_secs(2);

    /// Runs `argv` with `role`, once.
    pub fn new(argv: impl IntoIterator<Item = impl Into<String>>, role: Role) -> Self {
        Self {
            argv: argv.into_iter().map(Into::into).collect(),
            role,
            restart: false,
        }
    }

    /// Sets whether it is started again when it exits.
    pub fn restart(mut self, restart: bool) -> Self {
        self.restart = restart;
        self
    }

    /// Whether a client started at `started` and exited at `now` comes back.
    pub fn restarts_after(&self, started: Instant, now: Instant) -> bool {
        self.restart && now.duration_since(started) >= Self::MIN_UPTIME
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panels_sit_on_their_edge_and_reserve_it() {
        let top = Role::Panel {
            edge: Edge::Top,
            size: 32,
            keyboard: false,
        };
        let right = Role::Panel {
            edge: Edge::Right,
            size: 48,
            keyboard: true,
        };
        assert_eq!(
            top.geometry((1280, 800)),
            Some(Geometry::new((0, 0).into(), (1280, 32).into()))
        );
        assert_eq!(
            right.geometry((1280, 800)),
            Some(Geometry::new((1232, 0).into(), (48, 800).into()))
        );
        assert_eq!(
            Role::Overlay.geometry((1280, 800)),
            Some(Geometry::new((0, 0).into(), (1280, 800).into()))
        );
        assert_eq!(Role::App.geometry((1280, 800)), None);

        let reserved = Reserved::of([top, right, Role::Overlay, Role::App, top]);
        assert_eq!(
            reserved,
            Reserved {
                top: 64,
                right: 48,
                ..Reserved::default()
            }
        );
        let bounds = Geometry::new((0, 0).into(), (1280, 800).into());
        assert_eq!(
            reserved.shrink(bounds),
            Geometry::new((0, 64).into(), (1232, 736).into())
        );
    }

    #[test]
    fn a_panel_never_outgrows_the_output() {
        let huge = Role::Panel {
            edge: Edge::Bottom,
            size: 5000,
            keyboard: false,
        };
        assert_eq!(
            huge.geometry((640, 480)),
            Some(Geometry::new((0, 0).into(), (640, 480).into()))
        );
    }

    #[test]
    fn crash_loops_are_not_restarted() {
        let client = RuntimeClient::new(["panel"], Role::Overlay).restart(true);
        let start = Instant::now();
        assert!(!client.restarts_after(start, start + Duration::from_millis(500)));
        assert!(client.restarts_after(start, start + Duration::from_secs(3)));
        assert!(
            !RuntimeClient::new(["x"], Role::App)
                .restarts_after(start, start + Duration::from_secs(9))
        );
    }
}

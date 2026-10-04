use mcsapi::{Geometry, WindowId};
use mcsapi_compositor::{Compositor, Placement, Shell};

struct Empty;

impl Shell for Empty {
    fn map_window(&mut self, _app_id: &str, _title: &str) -> WindowId {
        WindowId::new(1).unwrap()
    }
    fn unmap_window(&mut self, _window: WindowId) {}
    fn set_output(&mut self, _size: (i32, i32)) {}
    fn focused(&self) -> Option<WindowId> {
        None
    }
    fn placements(&self) -> Vec<Placement> {
        Vec::new()
    }
}

#[test]
fn undecorated_content_fills_the_frame() {
    let frame = Geometry::new((10, 20).into(), (300, 200).into());
    let p = Placement::undecorated(WindowId::new(3).unwrap(), frame, true);
    assert_eq!(p.client, frame);
    assert!(p.focused);
}

#[test]
fn remote_jobs_fail_once_the_compositor_is_gone() {
    let mut compositor = Compositor::new(Empty).launch("foot");
    let remote = compositor.remote();
    let clone = remote.clone();
    drop(compositor);
    assert!(!remote.run(|_shell: &mut Empty| {}));
    assert!(!clone.run(|_shell: &mut Empty| {}));
}

#[test]
fn frames_follow_the_refresh_rate() {
    use std::time::Duration;

    use mcsapi_compositor::OutputTiming;

    let hz = |hz: u32, vrr: bool| OutputTiming {
        refresh_mhz: hz * 1000,
        vrr,
    };
    assert_eq!(OutputTiming::default(), hz(60, false));
    assert_eq!(
        hz(144, false).refresh_interval(),
        Duration::from_nanos(6_944_444)
    );
    // Never faster than the display refreshes.
    assert_eq!(
        hz(60, true).interval_for(240),
        hz(60, true).refresh_interval()
    );
    // Without VRR, a whole number of refreshes at or under the target rate.
    assert_eq!(
        hz(60, false).interval_for(30),
        hz(60, false).refresh_interval() * 2
    );
    assert_eq!(
        hz(120, false).interval_for(30),
        hz(120, false).refresh_interval() * 4
    );
    assert_eq!(
        hz(144, false).interval_for(30),
        hz(144, false).refresh_interval() * 5
    );
    assert_eq!(
        hz(165, false).interval_for(30),
        hz(165, false).refresh_interval() * 6
    );
    // With VRR, exactly the target.
    assert_eq!(
        hz(144, true).interval_for(30),
        Duration::from_nanos(33_333_333)
    );
    // Nonsense refresh rates fall back to something sane.
    assert!(hz(0, false).refresh_interval() <= Duration::from_secs(1));
}

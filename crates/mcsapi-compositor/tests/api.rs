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

use mcsapi::{Desktop, Error, Geometry, Layout, WindowId, WorkspaceId};

fn window(value: u64) -> WindowId {
    WindowId::new(value).unwrap()
}

fn workspace(value: u64) -> WorkspaceId {
    WorkspaceId::new(value).unwrap()
}

fn windows(count: u8) -> impl ExactSizeIterator<Item = WindowId> + Clone {
    (1..=count).map(|id| window(u64::from(id)))
}

fn desktop() -> Desktop {
    Desktop::new([workspace(1), workspace(2)]).unwrap()
}

fn bounds(width: i32, height: i32) -> Geometry {
    Geometry::new((-20, 10).into(), (width, height).into())
}

#[test]
fn identities_and_workspace_configuration_are_validated() {
    assert!(WindowId::new(0).is_none());
    assert!(WorkspaceId::new(0).is_none());
    assert_eq!(window(7).get(), 7);
    assert_eq!(workspace(7).to_string(), "7");
    assert!(matches!(Desktop::new([]), Err(Error::NoWorkspaces)));
    assert!(matches!(
        Desktop::new([workspace(1), workspace(1)]),
        Err(Error::DuplicateWorkspace(_))
    ));
    let desktop = Desktop::new([workspace(2), workspace(1)]).unwrap();
    assert_eq!(desktop.active().id(), workspace(2));
    assert!(
        desktop
            .workspaces()
            .map(|ws| ws.id())
            .eq([workspace(1), workspace(2)])
    );
}

#[test]
fn windows_have_unique_membership_and_stable_iterator_order() {
    let mut desktop = desktop();
    for id in [3, 2, 1] {
        desktop.insert(window(id)).unwrap();
    }
    assert!(
        desktop
            .active()
            .windows()
            .eq([window(3), window(1), window(2)])
    );
    let mut windows = desktop.active().windows();
    assert_eq!(windows.len(), 3);
    assert_eq!(windows.next(), Some(window(3)));
    assert_eq!(windows.len(), 2);
    assert_eq!(windows.clone().count(), 2);
    assert_eq!(windows.next(), Some(window(1)));
    assert_eq!(windows.next(), Some(window(2)));
    assert_eq!(windows.len(), 0);
    assert_eq!(windows.next(), None);
    assert_eq!(windows.next(), None);
    desktop.switch_to(workspace(2)).unwrap();
    assert_eq!(
        desktop.insert(window(1)),
        Err(Error::DuplicateWindow(window(1)))
    );
    assert_eq!(desktop.active().windows().len(), 0);
}

#[test]
fn focus_wraps_and_promoting_preserves_identity() {
    let mut desktop = desktop();
    assert_eq!(desktop.active_mut().focus_next(), None);
    assert_eq!(desktop.active_mut().focus_previous(), None);
    for id in 1..=3 {
        desktop.insert(window(id)).unwrap();
    }
    assert_eq!(desktop.active_mut().focus_next(), Some(window(1)));
    assert_eq!(desktop.active_mut().focus_previous(), Some(window(3)));
    desktop.active_mut().promote_focused();
    assert!(
        desktop
            .active()
            .windows()
            .eq([window(3), window(1), window(2)])
    );
    assert_eq!(desktop.active().focused(), Some(window(3)));
    assert_eq!(desktop.active_mut().focus_previous(), Some(window(2)));
    assert_eq!(
        desktop.active_mut().focus(window(9)),
        Err(Error::UnknownWindow(window(9)))
    );
    assert_eq!(desktop.active().focused(), Some(window(2)));
}

#[test]
fn moving_and_removing_windows_repair_focus() {
    let mut desktop = desktop();
    for id in 1..=3 {
        desktop.insert(window(id)).unwrap();
    }
    desktop.move_window(window(3), workspace(2)).unwrap();
    assert_eq!(desktop.active().id(), workspace(1));
    assert_eq!(desktop.active().focused(), Some(window(1)));
    desktop.remove(window(1)).unwrap();
    assert_eq!(desktop.active().focused(), Some(window(2)));
    desktop.remove(window(2)).unwrap();
    assert_eq!(desktop.active().focused(), None);
    desktop.switch_to(workspace(2)).unwrap();
    assert_eq!(desktop.active().focused(), Some(window(3)));
    desktop.move_window(window(3), workspace(2)).unwrap();
    assert_eq!(desktop.active().windows().len(), 1);
    desktop.remove(window(3)).unwrap();
    assert_eq!(desktop.active().focused(), None);
    assert_eq!(desktop.active().windows().next(), None);
}

#[test]
fn invalid_operations_leave_state_unchanged() {
    let mut desktop = desktop();
    desktop.insert(window(1)).unwrap();
    assert_eq!(
        desktop.switch_to(workspace(9)),
        Err(Error::UnknownWorkspace(workspace(9)))
    );
    assert_eq!(
        desktop.move_window(window(1), workspace(9)),
        Err(Error::UnknownWorkspace(workspace(9)))
    );
    assert_eq!(
        desktop.move_window(window(9), workspace(2)),
        Err(Error::UnknownWindow(window(9)))
    );
    assert_eq!(
        desktop.remove(window(9)),
        Err(Error::UnknownWindow(window(9)))
    );
    assert_eq!(desktop.active().id(), workspace(1));
    assert!(desktop.active().windows().eq([window(1)]));
    assert_eq!(desktop.active().focused(), Some(window(1)));
}

#[test]
fn tall_layout_is_lazy_and_distributes_all_pixels() {
    let mut placements = Layout::Tall.arrange(bounds(101, 101), windows(4)).unwrap();
    assert_eq!(placements.len(), 4);
    assert_eq!(placements.next().unwrap().geometry, bounds(50, 101));
    for (index, (y, height)) in [(10, 34), (44, 34), (78, 33)].into_iter().enumerate() {
        let placement = placements.next().unwrap();
        assert_eq!(placement.window, window(index as u64 + 2));
        assert_eq!(
            placement.geometry,
            Geometry::new((30, y).into(), (51, height).into())
        );
    }
    assert_eq!(placements.len(), 0);
    assert_eq!(placements.next(), None);
    assert_eq!(placements.next(), None);
}

#[test]
fn empty_single_and_monocle_layouts_are_well_defined() {
    assert_eq!(Layout::Tall.arrange(bounds(1, 1), []).unwrap().count(), 0);
    assert_eq!(
        Layout::Tall
            .arrange(bounds(1, 1), [window(1)])
            .unwrap()
            .next()
            .unwrap()
            .geometry,
        bounds(1, 1)
    );
    assert!(
        Layout::Monocle
            .arrange(bounds(1, 1), windows(20))
            .unwrap()
            .all(|p| p.geometry == bounds(1, 1))
    );
    let mut desktop = desktop();
    desktop.insert(window(1)).unwrap();
    desktop.active_mut().set_layout(Layout::Monocle);
    assert_eq!(desktop.active().layout(), Layout::Monocle);
}

#[test]
fn invalid_bounds_and_too_small_tiles_are_rejected() {
    assert!(matches!(
        Layout::Tall.arrange(bounds(0, 10), []),
        Err(Error::InvalidGeometry)
    ));
    assert!(matches!(
        Layout::Tall.arrange(bounds(1, 10), [window(1), window(2)]),
        Err(Error::InsufficientSpace)
    ));
    assert!(matches!(
        Layout::Tall.arrange(bounds(10, 1), windows(3)),
        Err(Error::InsufficientSpace)
    ));
    let overflowing = Geometry::new((i32::MAX, 0).into(), (1, 1).into());
    assert!(matches!(
        Layout::Tall.arrange(overflowing, []),
        Err(Error::InvalidGeometry)
    ));
    let overflowing = Geometry::new((0, i32::MAX).into(), (1, 1).into());
    assert!(matches!(
        Layout::Tall.arrange(overflowing, []),
        Err(Error::InvalidGeometry)
    ));
}

#[test]
fn extreme_valid_coordinates_do_not_overflow() {
    let bounds = Geometry::new((i32::MIN, i32::MIN).into(), (i32::MAX, i32::MAX).into());
    assert_eq!(Layout::Tall.arrange(bounds, windows(4)).unwrap().count(), 4);
}

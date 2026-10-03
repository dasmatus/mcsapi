use mcsapi_ui::egui::{self, Modifiers, Pos2, Vec2, pos2, vec2};
use mcsapi_ui::gesture::{
    EguiBridge, GestureEvent, GesturePhase, GestureSettings, GestureTracker, Transform,
};

const FRAME: f64 = 1.0 / 60.0;

fn close(a: Pos2, b: Pos2) -> bool {
    (a - b).length() < 1e-3
}

/// Two-finger swipe moving `step` per frame for `frames` frames, still down.
fn swipe(tracker: &mut GestureTracker, step: Vec2, frames: usize) -> f64 {
    let at = pos2(200.0, 200.0);
    tracker.handle(GestureEvent::SwipeBegin { fingers: 2 }, at, 0.0);
    let mut time = 0.0;
    for _ in 0..frames {
        time += FRAME;
        tracker.handle(GestureEvent::SwipeUpdate { delta: step }, at, time);
    }
    time
}

#[test]
fn swipes_move_content_exactly_with_the_fingers() {
    let mut tracker = GestureTracker::default();
    swipe(&mut tracker, vec2(3.0, -2.0), 10);
    assert_eq!(tracker.transform().translation, vec2(30.0, -20.0));
    assert_eq!(tracker.phase(), GesturePhase::Tracking { fingers: 2 });
}

#[test]
fn pinches_and_rotations_keep_the_point_under_the_fingers() {
    let mut tracker = GestureTracker::default();
    let fingers = pos2(120.0, 80.0);
    tracker.handle(GestureEvent::SwipeBegin { fingers: 2 }, fingers, 0.0);
    tracker.handle(
        GestureEvent::SwipeUpdate {
            delta: vec2(15.0, 5.0),
        },
        fingers,
        0.01,
    );
    tracker.handle(GestureEvent::SwipeEnd { cancelled: true }, fingers, 0.02);
    let under = tracker.transform().invert(fingers);

    tracker.handle(GestureEvent::PinchBegin { fingers: 2 }, fingers, 1.0);
    for (i, scale) in [1.2, 1.5, 0.7, 2.5].into_iter().enumerate() {
        tracker.handle(
            GestureEvent::PinchUpdate {
                delta: Vec2::ZERO,
                scale,
                rotation: 0.1,
            },
            fingers,
            1.0 + i as f64 * FRAME,
        );
        assert!(close(tracker.transform().apply(under), fingers));
    }
    let t = tracker.transform();
    assert!(
        (t.scale - 2.5).abs() < 1e-4,
        "scale is relative to the begin"
    );
    assert!((t.rotation - 0.4).abs() < 1e-5);
}

#[test]
fn pinch_movement_carries_the_content_along() {
    let mut tracker = GestureTracker::default();
    let start = pos2(50.0, 50.0);
    tracker.handle(GestureEvent::PinchBegin { fingers: 2 }, start, 0.0);
    tracker.handle(
        GestureEvent::PinchUpdate {
            delta: vec2(10.0, 0.0),
            scale: 2.0,
            rotation: 0.0,
        },
        start + vec2(10.0, 0.0),
        FRAME,
    );
    // The content point first under the fingers followed them.
    assert!(close(
        tracker.transform().apply(start),
        start + vec2(10.0, 0.0)
    ));
}

#[test]
fn transforms_invert() {
    let t = Transform {
        translation: vec2(13.0, -7.0),
        scale: 1.7,
        rotation: 0.9,
    };
    let p = pos2(42.0, 17.0);
    assert!(close(t.invert(t.apply(p)), p));
}

#[test]
fn lifting_fingers_mid_motion_coasts_and_slows_down() {
    let mut tracker = GestureTracker::default();
    let time = swipe(&mut tracker, vec2(10.0, 0.0), 10);
    let at = pos2(200.0, 200.0);
    tracker.handle(GestureEvent::SwipeEnd { cancelled: false }, at, time);
    assert_eq!(tracker.phase(), GesturePhase::Coasting);
    assert!(tracker.is_animating());

    let released = tracker.transform().translation.x;
    let mut last_step = f32::INFINITY;
    let mut now = time;
    let mut previous = released;
    while tracker.is_animating() {
        now += FRAME;
        tracker.tick(now);
        let x = tracker.transform().translation.x;
        let step = x - previous;
        assert!(step >= 0.0 && step <= last_step, "momentum only slows down");
        last_step = step;
        previous = x;
        assert!(now < time + 10.0, "momentum stops");
    }
    // 600 pt/s with friction 2.5 coasts about 600 / 2.5 = 240 pt.
    let coasted = previous - released;
    assert!((200.0..=245.0).contains(&coasted), "coasted {coasted}");
    assert_eq!(tracker.phase(), GesturePhase::Idle);
}

#[test]
fn resting_before_lifting_or_cancelling_stops_dead() {
    let at = pos2(200.0, 200.0);
    let mut paused = GestureTracker::default();
    let time = swipe(&mut paused, vec2(10.0, 0.0), 10);
    paused.handle(GestureEvent::SwipeEnd { cancelled: false }, at, time + 0.3);
    assert_eq!(paused.phase(), GesturePhase::Idle);

    let mut cancelled = GestureTracker::default();
    let time = swipe(&mut cancelled, vec2(10.0, 0.0), 10);
    cancelled.handle(GestureEvent::SwipeEnd { cancelled: true }, at, time);
    assert_eq!(cancelled.phase(), GesturePhase::Idle);

    let mut off = GestureTracker::new(GestureSettings {
        momentum: false,
        ..GestureSettings::default()
    });
    let time = swipe(&mut off, vec2(10.0, 0.0), 10);
    off.handle(GestureEvent::SwipeEnd { cancelled: false }, at, time);
    assert_eq!(off.phase(), GesturePhase::Idle);
}

#[test]
fn resting_fingers_catch_coasting_content() {
    let at = pos2(200.0, 200.0);
    let mut tracker = GestureTracker::default();
    let time = swipe(&mut tracker, vec2(10.0, 0.0), 10);
    tracker.handle(GestureEvent::SwipeEnd { cancelled: false }, at, time);
    tracker.tick(time + 0.1);
    tracker.handle(GestureEvent::HoldBegin { fingers: 2 }, at, time + 0.1);
    let caught = tracker.transform();
    tracker.handle(GestureEvent::HoldEnd { cancelled: false }, at, time + 0.5);
    tracker.tick(time + 1.0);
    assert_eq!(tracker.transform(), caught);
    assert_eq!(tracker.phase(), GesturePhase::Idle);
}

#[test]
fn scale_limits_hold_without_drifting_the_pivot() {
    let mut tracker = GestureTracker::new(GestureSettings {
        scale_range: 0.5..=2.0,
        ..GestureSettings::default()
    });
    let fingers = pos2(30.0, 40.0);
    tracker.handle(GestureEvent::PinchBegin { fingers: 2 }, fingers, 0.0);
    tracker.handle(
        GestureEvent::PinchUpdate {
            delta: Vec2::ZERO,
            scale: 10.0,
            rotation: 0.0,
        },
        fingers,
        FRAME,
    );
    let t = tracker.transform();
    assert_eq!(t.scale, 2.0);
    assert!(close(t.apply(fingers), fingers));
}

#[test]
fn invalid_scale_ranges_never_panic() {
    for scale_range in [2.0..=1.0, f32::NAN..=f32::NAN, -1.0..=0.0] {
        let mut tracker = GestureTracker::new(GestureSettings {
            scale_range,
            ..GestureSettings::default()
        });
        let at = Pos2::ZERO;
        tracker.handle(GestureEvent::SwipeBegin { fingers: 2 }, at, 0.0);
        tracker.handle(
            GestureEvent::SwipeUpdate {
                delta: vec2(5.0, 0.0),
            },
            at,
            FRAME,
        );
        tracker.handle(GestureEvent::PinchBegin { fingers: 2 }, at, 1.0);
        tracker.handle(
            GestureEvent::PinchUpdate {
                delta: Vec2::ZERO,
                scale: 3.0,
                rotation: 0.0,
            },
            at,
            1.0 + FRAME,
        );
        let t = tracker.transform();
        assert!(t.scale.is_finite() && t.scale > 0.0);
        assert!(t.translation.x.is_finite());
    }
}

#[test]
fn rotation_can_be_turned_off() {
    let mut tracker = GestureTracker::new(GestureSettings {
        rotate: false,
        ..GestureSettings::default()
    });
    let at = Pos2::ZERO;
    tracker.handle(GestureEvent::PinchBegin { fingers: 2 }, at, 0.0);
    tracker.handle(
        GestureEvent::PinchUpdate {
            delta: Vec2::ZERO,
            scale: 1.0,
            rotation: 1.0,
        },
        at,
        FRAME,
    );
    assert_eq!(tracker.transform().rotation, 0.0);
}

#[test]
fn updates_without_a_begin_move_but_never_coast() {
    let mut tracker = GestureTracker::default();
    let at = Pos2::ZERO;
    for i in 1..=5 {
        tracker.handle(
            GestureEvent::SwipeUpdate {
                delta: vec2(0.0, 20.0),
            },
            at,
            i as f64 * FRAME,
        );
    }
    tracker.handle(GestureEvent::SwipeEnd { cancelled: false }, at, 0.1);
    assert_eq!(tracker.transform().translation, vec2(0.0, 100.0));
    assert_eq!(tracker.phase(), GesturePhase::Idle);
}

/// Runs one frame of a pannable surface fed by `events`.
fn frame(
    ctx: &egui::Context,
    tracker: &mut GestureTracker,
    time: f64,
    events: Vec<egui::Event>,
) -> bool {
    let mut changed = false;
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))),
        time: Some(time),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        changed = tracker.update(ui, ui.max_rect());
    });
    output.textures_delta.clear();
    changed
}

#[test]
fn egui_apps_follow_bridged_gestures_and_coast() {
    let ctx = egui::Context::default();
    let mut tracker = GestureTracker::default();
    let mut bridge = EguiBridge::default();
    let mods = Modifiers::NONE;
    let pointer = pos2(100.0, 100.0);

    frame(
        &ctx,
        &mut tracker,
        0.0,
        vec![egui::Event::PointerMoved(pointer)],
    );
    let mut time = 0.0;
    let mut events = bridge.events(GestureEvent::PinchBegin { fingers: 2 }, mods);
    for i in 1..=8 {
        time += FRAME;
        events.extend(bridge.events(
            GestureEvent::PinchUpdate {
                delta: vec2(8.0, 0.0),
                scale: 1.0 + 0.1 * i as f32,
                rotation: 0.0,
            },
            mods,
        ));
        assert!(frame(&ctx, &mut tracker, time, std::mem::take(&mut events)));
    }
    let t = tracker.transform();
    assert!((t.scale - 1.8).abs() < 1e-4, "scale {}", t.scale);
    assert!(matches!(tracker.phase(), GesturePhase::Tracking { .. }));

    time += FRAME;
    let end = bridge.events(GestureEvent::PinchEnd { cancelled: false }, mods);
    frame(&ctx, &mut tracker, time, end);
    assert_eq!(tracker.phase(), GesturePhase::Coasting);
    let released = tracker.transform();
    time += FRAME;
    assert!(frame(&ctx, &mut tracker, time, Vec::new()));
    assert!(tracker.transform().translation.x > released.translation.x);
}

#[test]
fn egui_gestures_outside_the_area_are_ignored() {
    let ctx = egui::Context::default();
    let mut tracker = GestureTracker::default();
    let wheel = egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, 10.0),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::NONE,
    };
    let input = egui::RawInput {
        events: vec![egui::Event::PointerMoved(pos2(390.0, 290.0)), wheel],
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        let area = egui::Rect::from_min_size(Pos2::ZERO, vec2(50.0, 50.0));
        assert!(!tracker.update(ui, area));
    });
    output.textures_delta.clear();
}

#[test]
fn bridge_marks_gesture_boundaries_and_relative_zoom() {
    let mut bridge = EguiBridge::default();
    let mods = Modifiers::NONE;
    let phase = |events: &[egui::Event]| match events {
        [egui::Event::MouseWheel { phase, .. }, ..] => Some(*phase),
        _ => None,
    };
    assert_eq!(
        phase(&bridge.events(GestureEvent::HoldBegin { fingers: 3 }, mods)),
        Some(egui::TouchPhase::Start)
    );
    assert_eq!(
        phase(&bridge.events(GestureEvent::HoldEnd { cancelled: false }, mods)),
        Some(egui::TouchPhase::Cancel),
        "resting fingers never start momentum"
    );
    bridge.events(GestureEvent::PinchBegin { fingers: 2 }, mods);
    let zoom = |events: Vec<egui::Event>| {
        events.into_iter().find_map(|e| match e {
            egui::Event::Zoom(z) => Some(z),
            _ => None,
        })
    };
    let update = |scale| GestureEvent::PinchUpdate {
        delta: Vec2::ZERO,
        scale,
        rotation: 0.0,
    };
    assert_eq!(zoom(bridge.events(update(2.0), mods)), Some(2.0));
    assert_eq!(zoom(bridge.events(update(3.0), mods)), Some(1.5));
}

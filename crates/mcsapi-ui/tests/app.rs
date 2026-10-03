use mcsapi_ui::{App, Theme, egui, run_frame};

#[derive(Default)]
struct Counter {
    frames: u32,
}

impl App for Counter {
    fn title(&self) -> &str {
        "Counter"
    }

    fn ui(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        self.frames += 1;
        ui.label(egui::RichText::new(format!("frame {}", self.frames)).color(theme.foreground));
    }
}

#[test]
fn run_frame_draws_the_app_once_per_frame() {
    let context = egui::Context::default();
    let mut app = Counter::default();
    for _ in 0..2 {
        let mut output = run_frame(
            &mut app,
            &context,
            egui::RawInput::default(),
            &Theme::default(),
        );
        assert!(!output.shapes.is_empty());
        output.textures_delta.clear();
    }
    assert_eq!(app.frames, 2);
}

#[test]
fn apps_are_object_safe() {
    let mut app: Box<dyn App> = Box::new(Counter::default());
    assert_eq!(app.title(), "Counter");
    run_frame(
        app.as_mut(),
        &egui::Context::default(),
        egui::RawInput::default(),
        &Theme::default(),
    )
    .textures_delta
    .clear();
}

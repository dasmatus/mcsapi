//! The SVG icons Zed's `ui` components draw, as a GPUI asset source.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

/// Serves the icons Zed's `ui` components name (`icons/*.svg`) from the
/// binary, ahead of none or after an app's own assets.
///
/// GPUI takes one asset source, when the application is created, so pass this
/// there; without it checkboxes, carets and every other icon draw nothing:
///
/// ```ignore
/// gpui_platform::application()
///     .with_assets(mcsapi_components_gpui::Assets::new())
///     .run(|cx| { /* ... */ });
/// ```
#[derive(Default)]
pub struct Assets {
    app: Option<Box<dyn AssetSource>>,
}

impl Assets {
    /// Only the embedded icons.
    pub fn new() -> Self {
        Self::default()
    }

    /// The app's own assets, then the embedded icons for any path they do not
    /// have, so an app can replace an icon by shipping the same path.
    pub fn with_app_assets(app: impl AssetSource) -> Self {
        Self {
            app: Some(Box::new(app)),
        }
    }
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(app) = &self.app
            && let Some(bytes) = app.load(path)?
        {
            return Ok(Some(bytes));
        }
        Ok(icons::embedded_svg(path).map(Cow::Borrowed))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = match &self.app {
            Some(app) => app.list(path)?,
            None => Vec::new(),
        };
        paths.extend(
            icons::embedded_svg_paths(path)
                .filter(|name| !paths.iter().any(|known| known.as_ref() == *name))
                .map(SharedString::new_static)
                .collect::<Vec<_>>(),
        );
        Ok(paths)
    }
}

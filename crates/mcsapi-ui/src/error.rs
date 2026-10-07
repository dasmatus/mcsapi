//! Errors an app shows the person using it, and the documentation that
//! explains them.
//!
//! [`Error`] is a [`miette::Report`] plus an optional [`DocLink`], a section of
//! the desktop's documentation. A component draws it (`mcsapi-components`'
//! `ErrorAlert`) with the message, its causes, miette's help text and a
//! "Learn more" button that opens the section through [`Docs`]: the copy of
//! the documentation installed on the machine when there is one, the
//! published site otherwise.
//!
//! An error type names its section the way it names its code, with miette's
//! `url` attribute and the `docs:` scheme, so the pointer sits beside the
//! message it explains:
//!
//! ```
//! use mcsapi_ui::error::{DocLink, Error};
//!
//! #[derive(Debug, thiserror::Error, miette::Diagnostic)]
//! #[error("no theme named {0}")]
//! #[diagnostic(
//!     code(example::no_theme),
//!     help("pick one of the themes Settings lists"),
//!     url("docs:icon-theme#where-themes-come-from")
//! )]
//! struct NoTheme(String);
//!
//! let error = Error::new(NoTheme("pastel".into()));
//! assert_eq!(error.to_string(), "no theme named pastel");
//! assert_eq!(
//!     error.doc(),
//!     Some(&DocLink::new("icon-theme").section("where-themes-come-from"))
//! );
//! ```
//!
//! Errors from outside miette (`std::io::Error` and the like) come in through
//! [`Context`], which adds the sentence the person reads first and, if it
//! applies, the section:
//!
//! ```
//! use mcsapi_ui::error::{Context, DocLink, Result};
//!
//! fn open(path: &str) -> Result<String> {
//!     std::fs::read_to_string(path)
//!         .context(format!("Could not open {path}"))
//!         .doc(DocLink::new("layout").section("home"))
//! }
//!
//! let error = open("/nonexistent").unwrap_err();
//! assert_eq!(error.to_string(), "Could not open /nonexistent");
//! assert_eq!(error.causes().count(), 1);
//! ```

use std::{
    borrow::Cow,
    fmt,
    path::{Path, PathBuf},
};

use miette::Diagnostic;

/// miette, for apps that build diagnostics without depending on it
/// themselves, at the version this module's types use.
pub use miette;

/// A result whose error is shown to the person using the app.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// An error to show the person using the app: a miette report and the
/// documentation section that explains it, if any.
///
/// Like `anyhow::Error`, it is not itself an error type, so `?` turns any
/// [`Diagnostic`] into one.
pub struct Error {
    report: miette::Report,
    doc: Option<DocLink>,
}

impl Error {
    /// Wraps `diagnostic`, taking its documentation section from its
    /// `docs:` URL ([`DocLink::parse`]).
    pub fn new(diagnostic: impl Diagnostic + Send + Sync + 'static) -> Self {
        let doc = diagnostic
            .url()
            .and_then(|url| DocLink::parse(&url.to_string()));
        Self {
            report: miette::Report::new(diagnostic),
            doc,
        }
    }

    /// An error that is only a message, such as a refusal the app decided on
    /// itself.
    pub fn msg(message: impl fmt::Display + fmt::Debug + Send + Sync + 'static) -> Self {
        Self {
            report: miette::Report::msg(message),
            doc: None,
        }
    }

    /// Wraps an error from outside miette, such as an `std::io::Error`,
    /// with its message and causes as they are.
    pub fn plain(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::new(Plain(error))
    }

    /// Wraps a report built elsewhere, taking its section from its `docs:` URL.
    pub fn from_report(report: miette::Report) -> Self {
        let doc = report
            .url()
            .and_then(|url| DocLink::parse(&url.to_string()));
        Self { report, doc }
    }

    /// Points the error at `doc`, replacing any section it had.
    #[must_use]
    pub fn with_doc(mut self, doc: DocLink) -> Self {
        self.doc = Some(doc);
        self
    }

    /// Puts `message` in front, keeping this error as its cause and its
    /// section.
    #[must_use]
    pub fn context(self, message: impl fmt::Display + fmt::Debug + Send + Sync + 'static) -> Self {
        Self {
            report: self.report.wrap_err(message),
            doc: self.doc,
        }
    }

    /// The documentation section that explains this error.
    pub fn doc(&self) -> Option<&DocLink> {
        self.doc.as_ref()
    }

    /// The report: severity, code, help and the full chain.
    pub fn report(&self) -> &miette::Report {
        &self.report
    }

    /// The causes under the message, outermost first.
    pub fn causes(&self) -> impl Iterator<Item = &(dyn std::error::Error + 'static)> {
        self.report.chain().skip(1)
    }

    /// miette's help text: what the person can do about it.
    pub fn help(&self) -> Option<String> {
        self.report.help().map(|help| help.to_string())
    }

    /// The diagnostic code, such as `mcsapi_theme::not_found`.
    pub fn code(&self) -> Option<String> {
        self.report.code().map(|code| code.to_string())
    }

    /// Whether this is a warning rather than an error. Diagnostics without a
    /// severity are errors.
    pub fn is_warning(&self) -> bool {
        matches!(
            self.report.severity(),
            Some(miette::Severity::Warning | miette::Severity::Advice)
        )
    }

    /// The whole report as plain text, without colors or box drawing, for
    /// copying into a bug report or reading with a screen reader.
    pub fn details(&self) -> String {
        let mut text = String::new();
        // Writing into a String cannot fail.
        let _ =
            miette::NarratableReportHandler::new().render_report(&mut text, self.report.as_ref());
        if let Some(doc) = &self.doc {
            text.push_str(&format!("Documentation: {doc}\n"));
        }
        text
    }

    /// The report, dropping the section.
    pub fn into_report(self) -> miette::Report {
        self.report
    }
}

impl<D: Diagnostic + Send + Sync + 'static> From<D> for Error {
    fn from(diagnostic: D) -> Self {
        Self::new(diagnostic)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.report, f)
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.report, f)?;
        if let Some(doc) = &self.doc {
            write!(f, "\nDocumentation: {doc}")?;
        }
        Ok(())
    }
}

/// Turns other results into [`Result`]: a sentence in front and, optionally,
/// a documentation section.
pub trait Context<T> {
    /// Puts `message` in front of the error, keeping it as the cause.
    fn context(self, message: impl fmt::Display + fmt::Debug + Send + Sync + 'static) -> Result<T>;

    /// Points the error at `doc`.
    fn doc(self, doc: DocLink) -> Result<T>;
}

impl<T, E: std::error::Error + Send + Sync + 'static> Context<T> for std::result::Result<T, E> {
    fn context(self, message: impl fmt::Display + fmt::Debug + Send + Sync + 'static) -> Result<T> {
        self.map_err(|error| Error::plain(error).context(message))
    }

    fn doc(self, doc: DocLink) -> Result<T> {
        self.map_err(|error| Error::plain(error).with_doc(doc))
    }
}

impl<T> Context<T> for Result<T> {
    fn context(self, message: impl fmt::Display + fmt::Debug + Send + Sync + 'static) -> Result<T> {
        self.map_err(|error| error.context(message))
    }

    fn doc(self, doc: DocLink) -> Result<T> {
        self.map_err(|error| error.with_doc(doc))
    }
}

/// A plain error as a diagnostic with nothing but its message and source,
/// which is what miette's `IntoDiagnostic` does too, kept here so the
/// conversion needs no generic bound miette does not export.
#[derive(Debug)]
struct Plain<E>(E);

impl<E: fmt::Display> fmt::Display for Plain<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<E: std::error::Error> std::error::Error for Plain<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl<E: std::error::Error> Diagnostic for Plain<E> {}

/// A section of the desktop's documentation: a page, and optionally a
/// heading on it.
///
/// Pages are named as the documentation's source files are, without `.md`
/// (`halium` for `docs/halium.md`), and headings by their anchor, the
/// heading in lower case with spaces as dashes (`which-devices` for
/// "## Which devices"): the names the published site and its offline copy
/// both use.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DocLink {
    page: Cow<'static, str>,
    section: Option<Cow<'static, str>>,
}

impl DocLink {
    /// The URL scheme a diagnostic's `url` uses to name a section:
    /// `docs:page` or `docs:page#section`.
    pub const SCHEME: &'static str = "docs:";

    /// The top of `page`.
    pub fn new(page: impl Into<Cow<'static, str>>) -> Self {
        Self {
            page: page.into(),
            section: None,
        }
    }

    /// The heading `section` on this page.
    #[must_use]
    pub fn section(mut self, section: impl Into<Cow<'static, str>>) -> Self {
        self.section = Some(section.into());
        self
    }

    /// Reads `docs:page` or `docs:page#section`. Anything else, including a
    /// web URL a diagnostic links to instead, is not a section.
    pub fn parse(url: &str) -> Option<Self> {
        let rest = url.strip_prefix(Self::SCHEME)?;
        let (page, section) = match rest.split_once('#') {
            Some((page, section)) => (page, Some(section)),
            None => (rest, None),
        };
        if !is_name(page) || !section.is_none_or(is_name) {
            return None;
        }
        Some(Self {
            page: Cow::Owned(page.to_owned()),
            section: section.map(|s| Cow::Owned(s.to_owned())),
        })
    }

    /// The page.
    pub fn page(&self) -> &str {
        &self.page
    }

    /// The heading on the page, if one is named.
    pub fn heading(&self) -> Option<&str> {
        self.section.as_deref()
    }
}

impl fmt::Display for DocLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", Self::SCHEME, self.page)?;
        if let Some(section) = &self.section {
            write!(f, "#{section}")?;
        }
        Ok(())
    }
}

/// Whether `name` can be a page or heading name. Both become path
/// components of the offline copy, so nothing that could leave its
/// directory, and nothing a URL would need to escape beyond letters.
fn is_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Where the documentation is: a copy on this machine, the published site,
/// or both.
///
/// The offline copy is a directory with `sections/<page>.html` and
/// `sections/<page>/<heading>.html` for every page and heading, each a
/// redirect into the site itself. Plain files, so `xdg-open` opens them as
/// it would any page, while a `#heading` on a `file://` URL is lost on the
/// way to the browser.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Docs {
    local: Option<PathBuf>,
    online: Option<String>,
}

impl Docs {
    /// The variable naming the offline copy's directory.
    pub const LOCAL_VAR: &'static str = "MCSAPI_DOCS_DIR";
    /// The variable naming the published site's address.
    pub const ONLINE_VAR: &'static str = "MCSAPI_DOCS_URL";

    /// The documentation the OS names in [`Self::LOCAL_VAR`] and
    /// [`Self::ONLINE_VAR`]. With neither set there is none, and errors
    /// have no "Learn more".
    pub fn from_env() -> Self {
        let var = |name| std::env::var(name).ok().filter(|v| !v.is_empty());
        Self::new(
            var(Self::LOCAL_VAR).map(PathBuf::from),
            var(Self::ONLINE_VAR),
        )
    }

    /// An offline copy in `local`, the published site at `online`.
    pub fn new(local: Option<PathBuf>, online: Option<String>) -> Self {
        Self {
            local,
            online: online.map(|url| url.trim_end_matches('/').to_owned()),
        }
    }

    /// Whether there is no documentation at all, offline or online.
    pub fn is_empty(&self) -> bool {
        self.local.is_none() && self.online.is_none()
    }

    /// The address of `link`: in the offline copy when it has the page,
    /// on the published site otherwise, and `None` without either.
    ///
    /// A heading the offline copy lacks, because the page was rewritten
    /// since the error named it, falls back to the top of the page.
    pub fn url(&self, link: &DocLink) -> Option<String> {
        self.local_url(link).or_else(|| self.online_url(link))
    }

    fn local_url(&self, link: &DocLink) -> Option<String> {
        let sections = self.local.as_deref()?.join("sections");
        let page = sections.join(format!("{}.html", link.page));
        let heading = link
            .heading()
            .map(|h| sections.join(&*link.page).join(format!("{h}.html")));
        heading
            .filter(|path| path.is_file())
            .or_else(|| page.is_file().then_some(page))
            .map(|path| file_url(&path))
    }

    /// The address of `link` on the published site, whatever the offline
    /// copy has: the one to give a person who will read it on another
    /// device.
    pub fn online_url(&self, link: &DocLink) -> Option<String> {
        let base = self.online.as_deref()?;
        // The index page is the site's root.
        let page = if link.page == "index" { "" } else { &link.page };
        Some(match link.heading() {
            Some(heading) => format!("{base}/{page}#{heading}"),
            None => format!("{base}/{page}"),
        })
    }

    /// Opens `link` in the default browser with `xdg-open`, without waiting
    /// for it. `Ok(false)` when there is no documentation to open.
    pub fn open(&self, link: &DocLink) -> std::io::Result<bool> {
        let Some(url) = self.url(link) else {
            return Ok(false);
        };
        std::process::Command::new("xdg-open")
            .arg(url)
            .stdin(std::process::Stdio::null())
            .spawn()
            // xdg-open hands the page over and exits; reap it so it does
            // not linger as a zombie for the app's lifetime.
            .map(|mut child| {
                std::thread::spawn(move || child.wait());
                true
            })
    }
}

/// `path` as a `file://` URL, escaping what a URL path cannot hold.
fn file_url(path: &Path) -> String {
    let mut url = String::from("file://");
    for byte in path.as_os_str().as_encoded_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                url.push(char::from(*byte));
            }
            _ => url.push_str(&format!("%{byte:02X}")),
        }
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_docs_urls_only() {
        assert_eq!(DocLink::parse("docs:halium"), Some(DocLink::new("halium")));
        assert_eq!(
            DocLink::parse("docs:halium#which-devices"),
            Some(DocLink::new("halium").section("which-devices"))
        );
        assert_eq!(DocLink::parse("https://example.org/halium"), None);
        assert_eq!(DocLink::parse("docs:../etc/passwd"), None);
        assert_eq!(DocLink::parse("docs:a/b"), None);
        assert_eq!(DocLink::parse("docs:"), None);
        assert_eq!(DocLink::parse("docs:halium#"), None);
    }

    #[test]
    fn online_urls_follow_the_site() {
        let docs = Docs::new(None, Some("https://example.org/site/".into()));
        assert_eq!(
            docs.url(&DocLink::new("halium").section("which-devices"))
                .as_deref(),
            Some("https://example.org/site/halium#which-devices")
        );
        assert_eq!(
            docs.url(&DocLink::new("index")).as_deref(),
            Some("https://example.org/site/")
        );
        assert_eq!(Docs::default().url(&DocLink::new("halium")), None);
    }

    #[test]
    fn offline_copy_comes_first_and_falls_back_to_the_page() {
        let dir = std::env::temp_dir().join(format!("mcsapi-docs-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sections/halium")).unwrap();
        std::fs::write(dir.join("sections/halium.html"), "").unwrap();
        std::fs::write(dir.join("sections/halium/which-devices.html"), "").unwrap();
        let docs = Docs::new(Some(dir.clone()), Some("https://example.org".into()));

        let section = docs
            .url(&DocLink::new("halium").section("which-devices"))
            .unwrap();
        assert!(
            section.starts_with("file://")
                && section.ends_with("/sections/halium/which-devices.html")
        );
        let gone = docs
            .url(&DocLink::new("halium").section("renamed"))
            .unwrap();
        assert!(gone.ends_with("/sections/halium.html"));
        // A page the copy does not have yet is read online.
        assert_eq!(
            docs.url(&DocLink::new("newer")).as_deref(),
            Some("https://example.org/newer")
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn context_keeps_help_code_and_section_of_a_diagnostic() {
        let diagnostic = miette::MietteDiagnostic::new("no theme named pastel")
            .with_code("mcsapi_theme::not_found")
            .with_help("pick another")
            .with_url("docs:icon-theme");
        let error = Error::new(diagnostic).context("Could not apply the theme");
        assert_eq!(error.to_string(), "Could not apply the theme");
        assert_eq!(
            error.causes().next().unwrap().to_string(),
            "no theme named pastel"
        );
        assert_eq!(error.help().as_deref(), Some("pick another"));
        assert_eq!(error.code().as_deref(), Some("mcsapi_theme::not_found"));
        assert_eq!(error.doc(), Some(&DocLink::new("icon-theme")));
        assert!(!error.is_warning());
    }

    #[test]
    fn file_urls_escape_spaces() {
        assert_eq!(file_url(Path::new("/a b/c.html")), "file:///a%20b/c.html");
    }

    #[test]
    fn context_keeps_the_cause_and_the_section() {
        let error = std::fs::read("/nonexistent")
            .doc(DocLink::new("layout"))
            .context("Could not open /nonexistent")
            .unwrap_err();
        assert_eq!(error.to_string(), "Could not open /nonexistent");
        assert_eq!(error.doc(), Some(&DocLink::new("layout")));
        assert_eq!(error.causes().count(), 1);
        assert!(error.details().contains("Documentation: docs:layout"));
    }
}

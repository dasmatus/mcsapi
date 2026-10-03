//! Runtime that tracks which apps are installed and which instances are running.
//!
//! It is deliberately separate from the desktop policy (`mcsapi`) and the UI
//! toolkit (`mcsapi-ui`): an app may use any toolkit, and a compositor may host
//! apps without this runtime. This starter owns registration and instance
//! lifecycle only; process spawning, sandboxing, IPC, and mapping instances to
//! desktop windows are future work.
//!
//! ```
//! use mcsapi_runtime::{AppId, Manifest, Runtime};
//!
//! let mut runtime = Runtime::new();
//! let app = AppId::new("org.example.hello").unwrap();
//! runtime.register(Manifest::new(app.clone(), "Hello"))?;
//! let instance = runtime.launch(&app)?;
//! assert_eq!(runtime.instances().count(), 1);
//! runtime.stop(instance)?;
//! assert_eq!(runtime.instances().count(), 0);
//! # Ok::<(), mcsapi_runtime::Error>(())
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::{collections::BTreeMap, fmt, num::NonZeroU64};

/// A stable, reverse-DNS style app identifier such as `org.example.hello`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AppId(Box<str>);

impl AppId {
    /// Creates an identifier from ASCII letters, digits, `.`, `-`, and `_`.
    ///
    /// Returns `None` for an empty identifier, one with other characters, or
    /// one that starts or ends with `.` or contains `..`.
    pub fn new(id: &str) -> Option<Self> {
        let valid = !id.is_empty()
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
            && !id.starts_with('.')
            && !id.ends_with('.')
            && !id.contains("..");
        valid.then(|| Self(id.into()))
    }

    /// Returns the identifier text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AppId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Describes an installable app.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Manifest {
    /// The app's identifier.
    pub id: AppId,
    /// Human-readable name.
    pub name: String,
}

impl Manifest {
    /// Creates a manifest.
    pub fn new(id: AppId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
        }
    }
}

/// A runtime-assigned identity for one running copy of an app.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InstanceId(NonZeroU64);

impl InstanceId {
    /// Returns the numeric identity.
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl fmt::Display for InstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// An invalid runtime operation.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// An app with this identifier is already registered.
    DuplicateApp(AppId),
    /// No app with this identifier is registered.
    UnknownApp(AppId),
    /// The app still has running instances.
    AppRunning(AppId),
    /// No running instance has this identity.
    UnknownInstance(InstanceId),
    /// Instance identities are exhausted.
    InstancesExhausted,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateApp(id) => write!(f, "app already registered: {id}"),
            Self::UnknownApp(id) => write!(f, "unknown app: {id}"),
            Self::AppRunning(id) => write!(f, "app still running: {id}"),
            Self::UnknownInstance(id) => write!(f, "unknown instance: {id}"),
            Self::InstancesExhausted => f.write_str("instance identities exhausted"),
        }
    }
}

impl std::error::Error for Error {}

/// Registered apps and their running instances.
///
/// Instance identities are never reused within one runtime.
#[derive(Debug, Default)]
pub struct Runtime {
    apps: BTreeMap<AppId, Manifest>,
    instances: BTreeMap<InstanceId, AppId>,
    next: u64,
}

impl Runtime {
    /// Creates an empty runtime.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an app so it can be launched.
    pub fn register(&mut self, manifest: Manifest) -> Result<(), Error> {
        if self.apps.contains_key(&manifest.id) {
            return Err(Error::DuplicateApp(manifest.id));
        }
        self.apps.insert(manifest.id.clone(), manifest);
        Ok(())
    }

    /// Removes an app that has no running instances.
    pub fn unregister(&mut self, id: &AppId) -> Result<Manifest, Error> {
        if self.instances.values().any(|app| app == id) {
            return Err(Error::AppRunning(id.clone()));
        }
        self.apps
            .remove(id)
            .ok_or_else(|| Error::UnknownApp(id.clone()))
    }

    /// Returns a registered app's manifest.
    pub fn app(&self, id: &AppId) -> Option<&Manifest> {
        self.apps.get(id)
    }

    /// Registered apps, ordered by identifier.
    pub fn apps(&self) -> impl ExactSizeIterator<Item = &Manifest> {
        self.apps.values()
    }

    /// Starts a new instance of a registered app.
    pub fn launch(&mut self, id: &AppId) -> Result<InstanceId, Error> {
        if !self.apps.contains_key(id) {
            return Err(Error::UnknownApp(id.clone()));
        }
        let next = self.next.checked_add(1).ok_or(Error::InstancesExhausted)?;
        let instance = InstanceId(NonZeroU64::new(next).ok_or(Error::InstancesExhausted)?);
        self.next = next;
        self.instances.insert(instance, id.clone());
        Ok(instance)
    }

    /// Stops a running instance and returns its app.
    pub fn stop(&mut self, instance: InstanceId) -> Result<AppId, Error> {
        self.instances
            .remove(&instance)
            .ok_or(Error::UnknownInstance(instance))
    }

    /// Running instances with their apps, ordered by launch.
    pub fn instances(&self) -> impl ExactSizeIterator<Item = (InstanceId, &AppId)> {
        self.instances
            .iter()
            .map(|(&instance, app)| (instance, app))
    }
}

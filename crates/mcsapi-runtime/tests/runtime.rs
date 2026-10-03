use mcsapi_runtime::{AppId, Error, Manifest, Runtime};

fn app(id: &str) -> AppId {
    AppId::new(id).unwrap()
}

#[test]
fn app_ids_reject_malformed_text() {
    for bad in ["", ".a", "a.", "a..b", "a b", "a/b", "é"] {
        assert_eq!(AppId::new(bad), None, "{bad:?}");
    }
    assert_eq!(app("org.example-app_1").as_str(), "org.example-app_1");
}

#[test]
fn registration_is_unique() {
    let mut runtime = Runtime::new();
    runtime.register(Manifest::new(app("a"), "A")).unwrap();
    assert_eq!(
        runtime.register(Manifest::new(app("a"), "Again")),
        Err(Error::DuplicateApp(app("a")))
    );
    assert_eq!(runtime.app(&app("a")).unwrap().name, "A");
}

#[test]
fn launch_requires_a_registered_app() {
    let mut runtime = Runtime::new();
    assert_eq!(runtime.launch(&app("a")), Err(Error::UnknownApp(app("a"))));
}

#[test]
fn instances_are_distinct_and_never_reused() {
    let mut runtime = Runtime::new();
    runtime.register(Manifest::new(app("a"), "A")).unwrap();
    let first = runtime.launch(&app("a")).unwrap();
    let second = runtime.launch(&app("a")).unwrap();
    assert_ne!(first, second);
    assert_eq!(runtime.stop(first), Ok(app("a")));
    assert_eq!(runtime.stop(first), Err(Error::UnknownInstance(first)));
    let third = runtime.launch(&app("a")).unwrap();
    assert!(third > second);
    assert_eq!(
        runtime.instances().map(|(id, _)| id).collect::<Vec<_>>(),
        [second, third]
    );
}

#[test]
fn running_apps_cannot_be_unregistered() {
    let mut runtime = Runtime::new();
    runtime.register(Manifest::new(app("a"), "A")).unwrap();
    let instance = runtime.launch(&app("a")).unwrap();
    assert_eq!(
        runtime.unregister(&app("a")),
        Err(Error::AppRunning(app("a")))
    );
    runtime.stop(instance).unwrap();
    assert_eq!(runtime.unregister(&app("a")).unwrap().name, "A");
    assert_eq!(runtime.apps().len(), 0);
}

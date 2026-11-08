//! End-to-end actor lifecycle using the three packages built by plugin-tests.
use bongocat_plugin_manager::{PluginManager, PluginSource};
use bongocat_plugin_service::{PluginService, plugin_directory};
use std::{
    path::PathBuf,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires official packages prepared by just plugin-tests"]
fn current_companions_run_through_the_async_service_and_resume() {
    let root = PathBuf::from(
        std::env::var_os("BONGOCAT_PLUGIN_DEV_DIRECTORY").expect("development packages required"),
    );
    let source = PluginSource::Development {
        catalog: root.join("catalog.json"),
        packages: root,
    };
    let stop = AtomicBool::new(false);
    let offers = source.discover(env!("CARGO_PKG_VERSION"), &stop).unwrap();
    assert_eq!(offers.len(), 3);
    let store = tempfile::tempdir().unwrap();
    let mut manager = PluginManager::new(plugin_directory(store.path())).unwrap();
    for offer in &offers {
        manager
            .install(&offer.read(&stop).unwrap(), &offer.digest)
            .unwrap();
    }
    drop(manager);
    let (service, _) = PluginService::start(store.path(), 16).unwrap();
    let client = service.client();
    for offer in &offers {
        let center = client
            .set_enabled_blocking(&offer.metadata.id, true)
            .unwrap();
        assert!(
            center
                .cards
                .iter()
                .any(|card| card.id == offer.metadata.id && card.enabled && !card.failed)
        );
    }
    service.shutdown().unwrap();
    let (service, _) = PluginService::start(store.path(), 16).unwrap();
    let client = service.client();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let center = client.read_center_blocking().unwrap();
        let menus = service.menu_items();
        if center.cards.len() == 3
            && center.cards.iter().all(|card| card.enabled && !card.failed)
            && offers
                .iter()
                .all(|offer| menus.iter().any(|menu| menu.plugin == offer.metadata.id))
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "all enabled companions must resume"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    for offer in &offers {
        let center = client
            .set_enabled_blocking(&offer.metadata.id, false)
            .unwrap();
        assert!(
            center
                .cards
                .iter()
                .any(|card| card.id == offer.metadata.id && !card.enabled)
        );
    }
    service.shutdown().unwrap();
}

#[test]
#[ignore = "requires isolated frozen v1 and packaged v2 companions"]
fn real_companion_updates_keep_enable_choices_and_data() {
    let old = PathBuf::from(std::env::var_os("BONGOCAT_TEST_V1_BUNDLE").expect("v1 bundle"));
    let new = PathBuf::from(std::env::var_os("BONGOCAT_TEST_BUNDLE").expect("v2 bundle"));
    let source = |directory: PathBuf, digest: String| PluginSource::Production {
        owner: "unused".into(),
        repository: "unused".into(),
        public_key: String::new(),
        platform: "windows-x86_64".into(),
        bundled_directory: Some(directory),
        bundled_catalog_sha256: Some(digest),
    };
    let digest =
        bongocat_plugin_manager::sha256_of(&std::fs::read(old.join("plugins.json")).unwrap());
    let old_source = source(old, digest);
    let new_source = source(new, std::env::var("BONGOCAT_TEST_BUNDLE_DIGEST").unwrap());
    let store = tempfile::tempdir().unwrap();
    let plugin_dir = plugin_directory(store.path());
    let stop = AtomicBool::new(false);
    let mut manager = PluginManager::new(plugin_dir.clone()).unwrap();
    old_source
        .install_bundled(&mut manager, env!("CARGO_PKG_VERSION"), &stop)
        .unwrap();
    let old_versions: Vec<_> = manager
        .list_plugins()
        .iter()
        .map(|(metadata, _)| (metadata.id.clone(), metadata.version.clone()))
        .collect();
    let todo_dir = plugin_dir.join("data/com.bongocat.todo");
    let mail_dir = plugin_dir.join("data/com.bongocat.mail");
    std::fs::create_dir_all(&todo_dir).unwrap();
    std::fs::create_dir_all(&mail_dir).unwrap();
    std::fs::write(todo_dir.join("todo.json"), serde_json::to_vec(&serde_json::json!({"schema_version":1,"tasks":[{"id":"fixture-task","title":"Keep this task","completed":false,"priority":"low","dueDate":null,"createdAt":1,"updatedAt":1}]})).unwrap()).unwrap();
    let mail = serde_json::to_vec(
        &serde_json::json!({"schemaVersion":1,"accounts":[],"notifications":[]}),
    )
    .unwrap();
    std::fs::write(mail_dir.join("mail.json"), &mail).unwrap();
    manager.enable_plugin("com.bongocat.todo").unwrap();
    manager.shutdown().unwrap();
    drop(manager);

    let mail_offer = new_source
        .discover(env!("CARGO_PKG_VERSION"), &stop)
        .unwrap()
        .into_iter()
        .find(|offer| offer.metadata.id == "com.bongocat.mail")
        .unwrap();
    let mail_package = mail_offer.read(&stop).unwrap();
    let (service, _) =
        PluginService::start_with_source(store.path(), 16, new_source.clone(), None).unwrap();
    let client = service.client();
    let wait_for = |id: &str, version: &str| {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let center = client.read_center_blocking().unwrap();
            assert!(
                center.error.is_none(),
                "plugin center error: {:?}",
                center.error
            );
            if !center.loading
                && center.cards.iter().any(|card| {
                    card.id == id && card.version.as_deref() == Some(version) && !card.installing
                })
            {
                return center;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {id} {version}: {center:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    let initial = wait_for(
        "com.bongocat.todo",
        &old_versions
            .iter()
            .find(|(id, _)| id == "com.bongocat.todo")
            .unwrap()
            .1,
    );
    assert!(
        initial
            .cards
            .iter()
            .any(|card| card.id == "com.bongocat.todo" && card.enabled && card.update_available)
    );
    client
        .install_available_blocking("com.bongocat.todo")
        .unwrap();
    let updated = wait_for("com.bongocat.todo", "0.2.1");
    assert!(
        updated
            .cards
            .iter()
            .any(|card| card.id == "com.bongocat.todo" && card.enabled && !card.failed)
    );
    let local_update = client.install_local_blocking(mail_package.clone()).unwrap();
    assert!(
        local_update
            .cards
            .iter()
            .any(|card| card.id == "com.bongocat.mail"
                && card.version.as_deref() == Some("0.2.1")
                && !card.enabled
                && !card.failed)
    );
    service.shutdown().unwrap();
    assert_eq!(std::fs::read(mail_dir.join("mail.json")).unwrap(), mail);
    let todo: serde_json::Value =
        serde_json::from_slice(&std::fs::read(todo_dir.join("todo.json")).unwrap()).unwrap();
    assert_eq!(todo["tasks"][0]["title"], "Keep this task");
    for (id, version) in old_versions {
        assert!(plugin_dir.join("packages").join(id).join(version).is_dir());
    }
    let (service, _) =
        PluginService::start_with_source(store.path(), 16, new_source, None).unwrap();
    let client = service.client();
    let center = client.read_center_blocking().unwrap();
    assert!(
        center
            .cards
            .iter()
            .any(|card| card.id == "com.bongocat.todo" && card.enabled && !card.failed)
    );
    assert!(
        center
            .cards
            .iter()
            .any(|card| card.id == "com.bongocat.mail" && !card.enabled)
    );
    service.shutdown().unwrap();

    let fresh_store = tempfile::tempdir().unwrap();
    let (service, _) = PluginService::start(fresh_store.path(), 16).unwrap();
    let installed = service
        .client()
        .install_local_blocking(mail_package)
        .unwrap();
    assert!(
        installed
            .cards
            .iter()
            .any(|card| card.id == "com.bongocat.mail"
                && card.version.as_deref() == Some("0.2.1")
                && card.enabled
                && !card.failed)
    );
    service.shutdown().unwrap();
}

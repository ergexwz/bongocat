//! Acceptance against the actual payload extracted from a Windows installer.
use bongocat_plugin_manager::{PluginManager, PluginSource, PluginState};
use std::{path::PathBuf, sync::atomic::AtomicBool, time::Duration};

#[test]
#[cfg(target_os = "windows")]
fn frozen_v1_companions_keep_their_hashes_and_run_without_rebuilding() {
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../resources/bundled-plugins");
    let catalog_bytes = std::fs::read(directory.join("plugins.json")).unwrap();
    let catalog: bongocat_plugin_manager::Catalog = serde_json::from_slice(&catalog_bytes).unwrap();
    let store = tempfile::tempdir().unwrap();
    let mut manager = PluginManager::new(store.path().into()).unwrap();
    for entry in catalog.plugins {
        let Some(package) = entry.downloads.get("windows-x86_64") else {
            continue;
        };
        let bytes = std::fs::read(directory.join(package)).unwrap();
        // Pin delivered bytes independently of the adjacent catalog so changing
        // both the package and its catalog cannot recompile away compatibility.
        let delivered = match entry.id.as_str() {
            "com.bongocat.todo" => {
                "0b0afb8afc33e335270dc61ad3f6a360eb5609161674301dfdc49ef6d237ca53"
            }
            "com.bongocat.mail" => {
                "a4fb4be6cfe3e78cdfca5da181b8ddea5b2d9d433424bd858e1ae5b1425d40a8"
            }
            "com.bongocat.cyberwork" => {
                "5a870fa8223042a393d8a12d5710af6ea57e16388d4847f03126e27c01b6c3f3"
            }
            unexpected => panic!("unexpected frozen v1 fixture: {unexpected}"),
        };
        assert_eq!(bongocat_plugin_manager::sha256_of(&bytes), delivered);
        assert_eq!(entry.protocol_version, 1);
        assert_eq!(
            bongocat_plugin_manager::sha256_of(&bytes),
            entry.sha256.unwrap()
        );
        manager.install_local(&bytes).unwrap();
        manager.enable_plugin(&entry.id).unwrap();
        manager.tick(200);
        manager.disable_plugin(&entry.id).unwrap();
    }
    assert_eq!(manager.list_plugins().len(), 3);
    manager.shutdown().unwrap();
}

#[test]
#[ignore = "requires official packages prepared by just plugin-tests"]
fn development_catalog_runs_all_local_companions() {
    let root = PathBuf::from(
        std::env::var_os("BONGOCAT_PLUGIN_DEV_DIRECTORY")
            .expect("required development catalog directory"),
    );
    let source = PluginSource::Development {
        catalog: root.join("catalog.json"),
        packages: root,
    };
    let stop = AtomicBool::new(false);
    let offers = source.discover(env!("CARGO_PKG_VERSION"), &stop).unwrap();
    let ids: std::collections::BTreeSet<_> = offers
        .iter()
        .map(|offer| offer.metadata.id.as_str())
        .collect();
    assert_eq!(
        ids,
        [
            "com.bongocat.todo",
            "com.bongocat.mail",
            "com.bongocat.cyberwork"
        ]
        .into_iter()
        .collect()
    );
    let directory = tempfile::tempdir().unwrap();
    let mut manager = PluginManager::new(directory.path().to_path_buf()).unwrap();
    for offer in &offers {
        let package = offer.read(&stop).unwrap();
        manager.install(&package, &offer.digest).unwrap();
    }
    for offer in &offers {
        manager.enable_plugin(&offer.metadata.id).unwrap();
    }
    for elapsed in [0, 200, 400] {
        manager.tick(elapsed);
        assert!(
            manager
                .list_plugins()
                .iter()
                .all(|(_, state)| *state == PluginState::Enabled)
        );
    }
    manager.shutdown().unwrap();
    drop(manager);
    let mut reopened = PluginManager::new(directory.path().to_path_buf()).unwrap();
    reopened.discover_plugins().unwrap();
    reopened.resume_enabled().unwrap();
    assert!(
        reopened
            .list_plugins()
            .iter()
            .all(|(_, state)| *state == PluginState::Enabled)
    );
    reopened.shutdown().unwrap();
}

fn installed_bundle() -> (tempfile::TempDir, PluginManager) {
    let bundle = PathBuf::from(std::env::var_os("BONGOCAT_TEST_BUNDLE").expect("extracted bundle"));
    let digest = std::env::var("BONGOCAT_TEST_BUNDLE_DIGEST").expect("build-time catalog digest");
    let source = PluginSource::Production {
        owner: "unused".into(),
        repository: "unused".into(),
        public_key: String::new(),
        platform: "windows-x86_64".into(),
        bundled_directory: Some(bundle),
        bundled_catalog_sha256: Some(digest),
    };
    let offers = source
        .discover(env!("CARGO_PKG_VERSION"), &AtomicBool::new(false))
        .unwrap();
    let ids: std::collections::BTreeSet<_> = offers
        .iter()
        .map(|offer| offer.metadata.id.as_str())
        .collect();
    assert_eq!(
        ids,
        [
            "com.bongocat.todo",
            "com.bongocat.mail",
            "com.bongocat.cyberwork"
        ]
        .into_iter()
        .collect()
    );
    let icon_cache = tempfile::tempdir().unwrap();
    for mut offer in offers {
        offer
            .cache_icon(icon_cache.path(), &AtomicBool::new(false))
            .unwrap();
        assert!(
            std::path::Path::new(&offer.icon_url).is_file(),
            "missing bundled icon for {}",
            offer.metadata.id
        );
    }
    let directory = tempfile::tempdir().expect("isolated plugin data");
    let mut manager = PluginManager::new(directory.path().to_path_buf()).unwrap();
    source
        .install_bundled(
            &mut manager,
            env!("CARGO_PKG_VERSION"),
            &AtomicBool::new(false),
        )
        .unwrap();
    (directory, manager)
}

#[test]
#[ignore = "requires BONGOCAT_TEST_BUNDLE and its build-time catalog digest"]
fn packaged_companions_install_disabled_and_run_from_extracted_packages() {
    #[cfg(target_os = "windows")]
    if std::env::var_os("BONGOCAT_REQUIRE_ELEVATED").is_some() {
        assert!(
            bongocat_platform::is_elevated().expect("query host token"),
            "the acceptance host must actually be elevated"
        );
    }
    let (_directory, mut manager) = installed_bundle();
    let installed = manager.list_plugins();
    assert_eq!(installed.len(), 3);
    assert!(
        installed
            .iter()
            .all(|(_, state)| *state == PluginState::Discovered)
    );
    for (plugin, _) in &installed {
        manager.enable_plugin(&plugin.id).unwrap();
    }
    // The same real-process test can be sampled for resources without adding
    // production-only switches or touching the user's installed plugin data.
    let seconds: u64 = std::env::var("BONGOCAT_TEST_SOAK_SECONDS")
        .map(|value| value.parse().expect("whole soak seconds"))
        .unwrap_or(3);
    assert!((1..=600).contains(&seconds));
    for elapsed in (0..=seconds * 1000).step_by(100) {
        manager.tick(elapsed);
        assert!(
            manager
                .list_plugins()
                .iter()
                .all(|(_, state)| *state == PluginState::Enabled)
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    manager.shutdown().unwrap();
}

#[test]
#[ignore = "requires isolated v1 and v2 bundled package directories"]
fn v1_to_v2_upgrade_preserves_business_data_and_enabled_intent() {
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
    let old_digest =
        bongocat_plugin_manager::sha256_of(&std::fs::read(old.join("plugins.json")).unwrap());
    let old_source = source(old, old_digest);
    let new_source = source(new, std::env::var("BONGOCAT_TEST_BUNDLE_DIGEST").unwrap());
    let directory = tempfile::tempdir().unwrap();
    let mut manager = PluginManager::new(directory.path().into()).unwrap();
    let stop = AtomicBool::new(false);
    old_source
        .install_bundled(&mut manager, env!("CARGO_PKG_VERSION"), &stop)
        .unwrap();
    let old_versions: Vec<_> = manager
        .list_plugins()
        .iter()
        .map(|(m, _)| (m.id.clone(), m.version.clone()))
        .collect();
    let fixtures = [
        (
            "com.bongocat.todo",
            "todo.json",
            serde_json::json!({"schema_version":1,"tasks":[{"id":"fixture-task","title":"Keep this task","completed":false,"priority":"low","dueDate":null,"createdAt":1,"updatedAt":1}]}),
        ),
        (
            "com.bongocat.cyberwork",
            "cyberwork.json",
            serde_json::json!({"schemaVersion":1,"scalePercent":100,"taps":123,"gameMs":4567,"daily":{}}),
        ),
        (
            "com.bongocat.mail",
            "mail.json",
            serde_json::json!({"schemaVersion":1,"accounts":[],"notifications":[]}),
        ),
    ];
    for (id, filename, contents) in &fixtures {
        let data = directory.path().join("data").join(id);
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join(filename), serde_json::to_vec(contents).unwrap()).unwrap();
    }
    for id in ["com.bongocat.todo", "com.bongocat.cyberwork"] {
        manager.enable_plugin(id).unwrap();
    }
    let mail_path = directory.path().join("data/com.bongocat.mail/mail.json");
    let mail_before = std::fs::read(&mail_path).unwrap();
    let offers = new_source
        .discover(env!("CARGO_PKG_VERSION"), &stop)
        .unwrap();
    assert_eq!(offers.len(), 3);
    for offer in offers {
        assert_eq!(offer.metadata.protocol_version, 2);
        let bytes = offer.read(&stop).unwrap();
        manager.install(&bytes, &offer.digest).unwrap();
        assert_eq!(
            manager.installed_version(&offer.metadata.id).as_deref(),
            Some("0.2.1")
        );
    }
    assert!(manager.plugin_is_enabled("com.bongocat.todo"));
    assert!(manager.plugin_is_enabled("com.bongocat.cyberwork"));
    assert!(!manager.plugin_is_enabled("com.bongocat.mail"));
    assert_eq!(std::fs::read(&mail_path).unwrap(), mail_before);
    for (id, version) in old_versions {
        assert!(
            directory
                .path()
                .join("packages")
                .join(id)
                .join(version)
                .is_dir()
        );
    }
    manager.shutdown().unwrap();
    let todo: serde_json::Value = serde_json::from_slice(
        &std::fs::read(directory.path().join("data/com.bongocat.todo/todo.json")).unwrap(),
    )
    .unwrap();
    let cyber: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            directory
                .path()
                .join("data/com.bongocat.cyberwork/cyberwork.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(todo["tasks"][0]["title"], "Keep this task");
    assert_eq!(cyber["taps"], 123);
    assert_eq!(cyber["gameMs"], 4567);
    drop(manager);
    let mut reopened = PluginManager::new(directory.path().into()).unwrap();
    reopened.discover_plugins().unwrap();
    reopened.resume_enabled().unwrap();
    assert!(reopened.plugin_is_enabled("com.bongocat.todo"));
    assert!(reopened.plugin_is_enabled("com.bongocat.cyberwork"));
    assert!(!reopened.plugin_is_enabled("com.bongocat.mail"));
    reopened.shutdown().unwrap();
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires extracted installer and BONGOCAT_TEST_BROKER"]
fn packaged_broker_rejects_direct_invocation() {
    use std::{
        os::windows::process::CommandExt,
        process::{Command, Stdio},
    };
    let broker = PathBuf::from(std::env::var_os("BONGOCAT_TEST_BROKER").expect("packaged broker"));
    for arguments in [
        vec!["--help"],
        vec!["--internal-plugin-broker", "untrusted.exe", "."],
        vec!["--internal-plugin-broker-pipe", "invalid", "1"],
    ] {
        let status = Command::new(&broker)
            .args(arguments)
            .creation_flags(0x08000000)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(!status.success());
    }
}

#[test]
#[ignore = "requires isolated copied plugin store"]
fn copied_user_store_starts_cyberwork() {
    let root = PathBuf::from(std::env::var_os("BONGOCAT_TEST_COPIED_STORE").unwrap());
    let mut manager = PluginManager::new(root).unwrap();
    manager.discover_plugins().unwrap();
    manager
        .enable_plugin("com.bongocat.cyberwork")
        .expect("start copied cyberwork");
    manager.shutdown().unwrap();
}

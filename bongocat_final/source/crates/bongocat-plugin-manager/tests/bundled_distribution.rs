//! Acceptance against the actual payload extracted from a Windows installer.
use bongocat_plugin_manager::{PluginManager, PluginSource, PluginState};
use std::{path::PathBuf, sync::atomic::AtomicBool, time::Duration};

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

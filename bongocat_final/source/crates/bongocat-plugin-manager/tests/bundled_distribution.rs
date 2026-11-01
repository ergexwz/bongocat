//! Acceptance against the actual payload extracted from a Windows installer.
use bongocat_plugin_manager::{PluginManager, PluginSource, PluginState};
use std::{path::PathBuf, sync::atomic::AtomicBool, time::Duration};

#[test]
#[ignore = "requires BONGOCAT_TEST_BUNDLE and its build-time catalog digest"]
fn packaged_companions_install_disabled_and_run_from_extracted_packages() {
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
    let directory = tempfile::tempdir().expect("isolated plugin data");
    let mut manager = PluginManager::new(directory.path().to_path_buf()).unwrap();
    source
        .install_bundled(
            &mut manager,
            env!("CARGO_PKG_VERSION"),
            &AtomicBool::new(false),
        )
        .unwrap();
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
    for elapsed in (0..=3000).step_by(100) {
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

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

#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires extracted installer and BONGOCAT_TEST_BROKER"]
fn packaged_ordinary_broker_forwards_each_companions_protocol() {
    use bongocat_plugin_sdk::{HostMessage, Request, Response, read_frame, write_frame};
    use std::{
        os::windows::process::CommandExt,
        process::{Child, Command, Stdio},
        time::Instant,
    };
    struct Cleanup(Child);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let broker = PathBuf::from(std::env::var_os("BONGOCAT_TEST_BROKER").expect("packaged broker"));
    let (_directory, manager) = installed_bundle();
    // It must never act as another ordinary host or register startup on a
    // double-click. Its sole entry point is the private broker invocation.
    let status = Command::new(&broker)
        .arg("--help")
        .creation_flags(0x08000000)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(!status.success());
    for (plugin, _) in manager.list_plugins() {
        let entry = manager
            .plugin_dir()
            .join("packages")
            .join(&plugin.id)
            .join(&plugin.version)
            .join(format!("{}.exe", plugin.entry));
        let data = manager.data_root(&plugin.id, true).unwrap();
        let mut child = Cleanup(
            Command::new(&broker)
                .arg("--internal-plugin-broker")
                .arg(entry)
                .arg(data)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .creation_flags(0x08000000)
                .spawn()
                .unwrap(),
        );
        let mut input = child.0.stdin.take().unwrap();
        let mut output = child.0.stdout.take().unwrap();
        let (send, receive) = std::sync::mpsc::sync_channel(8);
        let reader = std::thread::spawn(move || {
            while let Ok(response) = read_frame::<Response>(&mut output) {
                if send.try_send(response).is_err() {
                    break;
                }
            }
        });
        for (index, message) in [
            HostMessage::Handshake {
                protocol_version: 1,
                id: plugin.id.clone(),
            },
            HostMessage::Start {
                config: Default::default(),
                state: Vec::new(),
            },
            HostMessage::Tick { elapsed_ms: 100 },
            HostMessage::Stop,
        ]
        .into_iter()
        .enumerate()
        {
            let sequence = index as u64 + 1;
            write_frame(&mut input, &Request { sequence, message }).unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                let response = receive
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .unwrap_or_else(|error| {
                        panic!("{} broker sequence {sequence}: {error}", plugin.id)
                    });
                if response.sequence == 0 {
                    continue;
                }
                assert_eq!(response.sequence, sequence);
                assert!(
                    response.result.is_ok(),
                    "{} broker refused a request",
                    plugin.id
                );
                break;
            }
        }
        drop(input);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < deadline, "broker shutdown deadline");
            std::thread::sleep(Duration::from_millis(20));
        }
        reader.join().unwrap();
    }
}

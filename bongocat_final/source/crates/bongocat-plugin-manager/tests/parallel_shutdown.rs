//! Real plugin processes wait for one another before accepting Stop.
use bongocat_plugin_manager::{PluginManager, PluginManifest};
use bongocat_plugin_sdk::{PluginIcon, PluginMetadata, PluginOutput};
use std::io::Write;

fn fixture_manager() -> (tempfile::TempDir, PluginManager) {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory
        .path()
        .join(format!("fixture{}", std::env::consts::EXE_SUFFIX));
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/parallel_stop.rs");
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    assert!(
        std::process::Command::new(rustc)
            .arg(fixture)
            .arg("-o")
            .arg(&executable)
            .status()
            .unwrap()
            .success()
    );
    let executable_bytes = std::fs::read(executable).unwrap();
    let store = directory.path().join("store");
    let mut manager = PluginManager::new(store).unwrap();
    for id in ["com.example.one", "com.example.two"] {
        let manifest = PluginManifest {
            metadata: PluginMetadata {
                id: id.into(),
                name: id.into(),
                description: "fixture".into(),
                version: "1.0.0".into(),
                app_versions: ">=2.3.1".into(),
                protocol_version: 1,
                entry: "fixture".into(),
                icon: PluginIcon::Emoji("X".into()),
                capabilities: vec![],
                localizations: Default::default(),
            },
        };
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
        zip.start_file("plugin.json", options).unwrap();
        zip.write_all(&serde_json::to_vec(&manifest).unwrap())
            .unwrap();
        zip.start_file(format!("fixture{}", std::env::consts::EXE_SUFFIX), options)
            .unwrap();
        zip.write_all(&executable_bytes).unwrap();
        zip.start_file("output.json", options).unwrap();
        zip.write_all(&serde_json::to_vec(&PluginOutput::default()).unwrap())
            .unwrap();
        manager
            .install_local(&zip.finish().unwrap().into_inner())
            .unwrap();
        manager.enable_plugin(id).unwrap();
    }
    (directory, manager)
}

#[test]
fn shutdown_stops_all_plugins_concurrently() {
    let (_directory, mut manager) = fixture_manager();
    manager.shutdown().unwrap();
}

#[test]
fn a_slow_plugin_does_not_block_another_plugin_and_queue_is_bounded() {
    use bongocat_plugin_manager::{PluginActor, PluginOperation};
    use std::time::{Duration, Instant};
    fn completion(actor: &PluginActor) -> Result<(), bongocat_plugin_manager::PluginManagerError> {
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            if let Ok(result) = actor.try_completion() {
                return result.result;
            }
            assert!(Instant::now() < deadline, "plugin completion deadline");
            std::thread::park_timeout(Duration::from_millis(1));
        }
    }
    let (_directory, mut manager) = fixture_manager();
    let slow = manager.spawn_actor("com.example.one").unwrap();
    let healthy = manager.spawn_actor("com.example.two").unwrap();
    slow.submit(1, PluginOperation::Action("slow".into()))
        .unwrap();
    slow.submit(0, PluginOperation::Preview("key".into(), "small".into()))
        .unwrap();
    // Replacing a pending preview must obey the byte limit too.
    assert_eq!(
        slow.submit(
            0,
            PluginOperation::Preview("key".into(), "x".repeat(1024 * 1024))
        )
        .unwrap_err()
        .kind(),
        std::io::ErrorKind::WouldBlock
    );
    // Fill the slow plugin's bounded mailbox without waiting for its IO.
    let mut rejected = false;
    for sequence in 2..=100 {
        if slow
            .submit(sequence, PluginOperation::Action("fast".into()))
            .is_err()
        {
            rejected = true;
            break;
        }
    }
    assert!(rejected);
    assert!(slow.diagnostics().rejected > 0);
    let mut latencies = Vec::new();
    for sequence in 101..201 {
        let started = Instant::now();
        healthy
            .submit(sequence, PluginOperation::Action("fast".into()))
            .unwrap();
        completion(&healthy).unwrap();
        latencies.push(started.elapsed());
    }
    latencies.sort_unstable();
    assert_eq!(healthy.diagnostics().completed, 100);
    if let Some(report) = std::env::var_os("BONGOCAT_LATENCY_REPORT") {
        std::fs::write(
            report,
            serde_json::to_vec_pretty(&serde_json::json!({
                "samples": latencies.len(),
                "p50_us": latencies[49].as_micros(),
                "p95_us": latencies[94].as_micros(),
                "p99_us": latencies[98].as_micros(),
                "max_us": latencies[99].as_micros(),
            }))
            .unwrap(),
        )
        .unwrap();
    }
    assert!(
        latencies[94] < Duration::from_millis(100),
        "healthy plugin must not wait for the slow plugin's 2s timeout"
    );
    assert!(completion(&slow).is_err());
    slow.request_stop();
    healthy.request_stop();
    // The failed child is already cleaned up; allow the healthy fixture to stop.
    std::fs::write(manager.plugin_dir().join("stop-com.example.one"), b"ready").unwrap();
    slow.join().unwrap();
    healthy.join().unwrap();
}

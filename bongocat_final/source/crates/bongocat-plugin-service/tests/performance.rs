//! Also copied into the baseline tree: only pre-existing public APIs are used.
use bongocat_plugin_manager::{PluginManager, PluginManifest};
use bongocat_plugin_sdk::{PluginIcon, PluginMetadata};
use bongocat_plugin_service::{PluginService, plugin_directory};
use std::{
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

fn file_totals(root: &std::path::Path) -> (u64, u64) {
    let mut total = (0, 0);
    for entry in std::fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let metadata = entry.path().symlink_metadata().unwrap();
        if metadata.is_dir() {
            let nested = file_totals(&entry.path());
            total.0 += nested.0;
            total.1 += nested.1;
        } else if metadata.is_file() {
            total.0 += 1;
            total.1 += metadata.len();
        }
    }
    total
}

#[test]
#[ignore = "run through tools/measure-plugin-performance.ps1 with isolated packages"]
fn isolated_plugin_resource_scenario() {
    let output =
        PathBuf::from(std::env::var_os("BONGOCAT_PERF_OUTPUT").expect("measurement output"));
    let count: usize = std::env::var("BONGOCAT_PERF_COUNT")
        .unwrap()
        .parse()
        .unwrap();
    assert!([0, 3, 10].contains(&count));
    let warmup: u64 = std::env::var("BONGOCAT_PERF_WARMUP")
        .unwrap()
        .parse()
        .unwrap();
    let seconds: u64 = std::env::var("BONGOCAT_PERF_SECONDS")
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=28_800).contains(&seconds) && warmup <= 300);
    let protocol: u32 = std::env::var("BONGOCAT_PERF_PROTOCOL")
        .unwrap()
        .parse()
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut manager = PluginManager::new(plugin_directory(directory.path())).unwrap();
    let mut ids = Vec::new();
    if let Some(fixture) = std::env::var_os("BONGOCAT_PERF_FIXTURE") {
        let binary = std::fs::read(fixture).unwrap();
        for n in 0..count {
            let id = format!("com.example.scaling-{n}");
            let manifest = PluginManifest {
                metadata: PluginMetadata {
                    id: id.clone(),
                    name: "Scaling fixture".into(),
                    description: "fixture".into(),
                    version: "1.0.0".into(),
                    app_versions: ">=2.5.3".into(),
                    protocol_version: protocol,
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
            zip.write_all(&binary).unwrap();
            manager
                .install_local(&zip.finish().unwrap().into_inner())
                .unwrap();
            ids.push(id);
        }
    } else if count > 0 {
        let packages = PathBuf::from(std::env::var_os("BONGOCAT_PERF_PACKAGES").unwrap());
        for file in std::fs::read_dir(packages).unwrap() {
            let path = file.unwrap().path();
            if path.extension().is_none_or(|s| s != "zip") {
                continue;
            }
            let bytes = std::fs::read(path).unwrap();
            let metadata = bongocat_plugin_manager::install::inspect_package(&bytes)
                .unwrap()
                .metadata;
            assert_eq!(metadata.protocol_version, protocol);
            ids.push(metadata.id);
            manager.install_local(&bytes).unwrap();
        }
        assert_eq!(ids.len(), count);
    }
    drop(manager);
    let (service, _) = PluginService::start(directory.path(), 32).unwrap();
    let client = service.client();
    for id in &ids {
        client.set_enabled_blocking(id, true).unwrap();
    }
    std::thread::sleep(Duration::from_secs(warmup));
    let mut micros = Vec::new();
    for n in 0..100 {
        let started = Instant::now();
        if ids.is_empty() {
            client.read_center_blocking().unwrap();
        } else {
            client
                .action_blocking(&ids[n % ids.len()], "benchmark-noop")
                .unwrap();
        }
        micros.push(started.elapsed().as_micros() as u64);
    }
    micros.sort_unstable();
    let (files, bytes) = file_totals(directory.path());
    let report = serde_json::json!({"ready": true, "pid": std::process::id(), "protocol": protocol, "plugins": count, "seconds": seconds, "request_p50_us": micros[49], "request_p95_us": micros[94], "request_p99_us": micros[98], "request_max_us": micros[99], "store_files_start": files, "store_bytes_start": bytes});
    std::fs::write(&output, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    let exercise = std::env::var_os("BONGOCAT_PERF_EXERCISE").is_some();
    let idle_seconds: u64 = std::env::var("BONGOCAT_PERF_IDLE_SECONDS")
        .unwrap_or_else(|_| "0".into())
        .parse()
        .unwrap();
    let restart_seconds: u64 = std::env::var("BONGOCAT_PERF_RESTART_SECONDS")
        .unwrap_or_else(|_| "60".into())
        .parse()
        .unwrap();
    let exercise_mode =
        std::env::var("BONGOCAT_PERF_EXERCISE_MODE").unwrap_or_else(|_| "mixed".into());
    assert!(["mixed", "actions", "restarts"].contains(&exercise_mode.as_str()));
    assert!(idle_seconds <= seconds && (1..=3600).contains(&restart_seconds));
    let mut cycles = 0;
    let mut actions = 0;
    if exercise && !ids.is_empty() {
        let until = Instant::now() + Duration::from_secs(seconds);
        std::thread::sleep(Duration::from_secs(idle_seconds));
        let mut next_restart = Instant::now() + Duration::from_secs(restart_seconds);
        while Instant::now() < until {
            if exercise_mode != "restarts" {
                for id in &ids {
                    client.action_blocking(id, "benchmark-noop").unwrap();
                    actions += 1;
                }
            }
            if exercise_mode != "actions" && Instant::now() >= next_restart {
                let id = &ids[cycles % ids.len()];
                client.set_enabled_blocking(id, false).unwrap();
                client.set_enabled_blocking(id, true).unwrap();
                cycles += 1;
                next_restart = Instant::now() + Duration::from_secs(restart_seconds);
                let center = client.read_center_blocking().unwrap();
                assert!(center.cards.iter().all(|card| card.enabled && !card.failed));
            }
            std::thread::sleep(
                Duration::from_secs(5).min(until.saturating_duration_since(Instant::now())),
            );
        }
    } else {
        std::thread::sleep(Duration::from_secs(seconds));
    }
    let center = client.read_center_blocking().unwrap();
    assert_eq!(center.cards.len(), count);
    assert!(center.cards.iter().all(|card| card.enabled && !card.failed));
    service.shutdown().unwrap();
    let mut report = report;
    report["passed"] = true.into();
    report["lifecycle_cycles"] = cycles.into();
    report["action_count"] = actions.into();
    report["exercise"] = exercise.into();
    report["exercise_mode"] = exercise_mode.into();
    report["idle_seconds"] = idle_seconds.into();
    report["restart_seconds"] = restart_seconds.into();
    let (files, bytes) = file_totals(directory.path());
    report["store_files_end"] = files.into();
    report["store_bytes_end"] = bytes.into();
    std::fs::write(output, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}

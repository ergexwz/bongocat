//! Cross-process v2 contract using the public SDK examples built by plugin-tests.
use bongocat_plugin_manager::{PluginActor, PluginManager, PluginManifest, PluginOperation};
use bongocat_plugin_sdk::{PluginOutput, UiNode};
use std::{
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

fn example_directory() -> PathBuf {
    PathBuf::from(std::env::var_os("BONGOCAT_V2_EXAMPLES").expect("v2 example binaries required"))
}
fn package(binary: &std::path::Path, id: &str, version: &str) -> Vec<u8> {
    // The rollback test deliberately pairs this declared identity with the
    // wrong executable, so select the manifest by identity rather than binary.
    let name = id.strip_prefix("com.example.").unwrap().replace('-', "_");
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../bongocat_plugins/crates/bongocat-plugin-sdk/examples/manifests")
        .join(format!("{name}.json"));
    let mut manifest: PluginManifest =
        serde_json::from_slice(&std::fs::read(path).expect("SDK example manifest is required"))
            .unwrap();
    assert_eq!(manifest.metadata.id, id);
    manifest.metadata.version = version.into();
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
    zip.start_file("plugin.json", options).unwrap();
    zip.write_all(&serde_json::to_vec(&manifest).unwrap())
        .unwrap();
    zip.start_file(
        format!(
            "{}{}",
            manifest.metadata.entry,
            std::env::consts::EXE_SUFFIX
        ),
        options,
    )
    .unwrap();
    zip.write_all(&std::fs::read(binary).expect("example was built"))
        .unwrap();
    zip.finish().unwrap().into_inner()
}
fn wait(actor: &PluginActor, sequence: u64) {
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        if let Ok(completion) = actor.try_completion() {
            assert_eq!(completion.sequence, sequence);
            completion.result.unwrap();
            return;
        }
        assert!(Instant::now() < deadline, "v2 request must complete");
        std::thread::park_timeout(Duration::from_millis(1));
    }
}
fn text(output: &PluginOutput) -> &str {
    match &output.model.nodes[0] {
        UiNode::Text { text } => text,
        _ => panic!("text output required"),
    }
}

#[test]
#[ignore = "requires SDK examples prepared by just plugin-tests"]
fn event_plugin_checkpoints_survive_restart_and_incompatible_update() {
    let examples = example_directory();
    let store = tempfile::tempdir().unwrap();
    let id = "com.example.event-counter";
    let mut manager = PluginManager::new(store.path().into()).unwrap();
    manager
        .install_local(&package(
            &examples.join(format!("event_counter{}", std::env::consts::EXE_SUFFIX)),
            id,
            "1.0.0",
        ))
        .unwrap();
    let actor = manager.spawn_actor(id).unwrap();
    actor.submit(1, PluginOperation::Enable(true)).unwrap();
    wait(&actor, 1);
    actor
        .submit(2, PluginOperation::Action("increment".into()))
        .unwrap();
    wait(&actor, 2);
    assert_eq!(text(&actor.take_projection().unwrap().unwrap().output), "1");
    let events = actor.take_events(256);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].id, "count-1");
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(actor.diagnostics().scheduled_wakes, 0);
    let session = store.path().join("data").join(id).join("session.json");
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&session).unwrap()).unwrap();
    assert_eq!(saved["state_schema_version"], 1);
    assert_eq!(saved["state"][0], 1);
    assert!(
        session
            .with_file_name("checkpoint-before-migration.json")
            .is_file()
    );
    actor.join().unwrap();
    drop(manager);
    let mut reopened = PluginManager::new(store.path().into()).unwrap();
    reopened.discover_plugins().unwrap();
    reopened.resume_enabled().unwrap();
    assert_eq!(text(reopened.plugin_output(id).unwrap()), "1");
    // A mismatched executable fails handshake after the current package is suspended.
    let wrong = package(
        &examples.join(format!("scheduled_timer{}", std::env::consts::EXE_SUFFIX)),
        id,
        "2.0.0",
    );
    assert!(reopened.install_local(&wrong).is_err());
    assert_eq!(reopened.installed_version(id).as_deref(), Some("1.0.0"));
    assert_eq!(text(reopened.plugin_output(id).unwrap()), "1");
    reopened.shutdown().unwrap();
}

#[test]
#[ignore = "requires SDK examples prepared by just plugin-tests"]
fn timer_requests_exactly_three_wakes_without_global_ticks() {
    let examples = example_directory();
    let store = tempfile::tempdir().unwrap();
    let id = "com.example.scheduled-timer";
    let mut manager = PluginManager::new(store.path().into()).unwrap();
    manager
        .install_local(&package(
            &examples.join(format!("scheduled_timer{}", std::env::consts::EXE_SUFFIX)),
            id,
            "1.0.0",
        ))
        .unwrap();
    let actor = manager.spawn_actor(id).unwrap();
    actor.submit(1, PluginOperation::Enable(true)).unwrap();
    wait(&actor, 1);
    let deadline = Instant::now() + Duration::from_secs(5);
    while actor.diagnostics().scheduled_wakes < 3 {
        assert!(Instant::now() < deadline);
        std::thread::park_timeout(Duration::from_millis(10));
    }
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut last = None;
    loop {
        if let Some(projection) = actor.take_projection() {
            last = projection;
        }
        if last
            .as_ref()
            .is_some_and(|projection| text(&projection.output) == "0")
        {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::park_timeout(Duration::from_millis(5));
    }
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(actor.diagnostics().scheduled_wakes, 3);
    actor.join().unwrap();
}

#[test]
#[ignore = "requires SDK examples prepared by just plugin-tests"]
fn hud_example_updates_from_subscribed_pet_context() {
    let examples = example_directory();
    let store = tempfile::tempdir().unwrap();
    let id = "com.example.companion-hud";
    let mut manager = PluginManager::new(store.path().into()).unwrap();
    manager
        .install_local(&package(
            &examples.join(format!("companion_hud{}", std::env::consts::EXE_SUFFIX)),
            id,
            "1.0.0",
        ))
        .unwrap();
    manager.enable_plugin(id).unwrap();
    assert_eq!(text(manager.plugin_output(id).unwrap()), "Pet visible");
    for visible in [false, true] {
        manager.pet_state(bongocat_plugin_sdk::PetState {
            model: None,
            visible,
        });
        assert_eq!(
            text(manager.plugin_output(id).unwrap()),
            if visible { "Pet visible" } else { "Pet hidden" }
        );
    }
    let actor = manager.spawn_actor(id).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let events = actor.take_events(256);
        if !events.is_empty() {
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].id, "visible-1");
            assert_eq!(events[0].title, "Pet visible");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "context notification must arrive"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(actor.diagnostics().scheduled_wakes, 0);
    actor.join().unwrap();
}

#[test]
#[ignore = "requires SDK examples prepared by just plugin-tests"]
fn full_event_queues_resume_without_losing_or_duplicating_events() {
    let examples = example_directory();
    let store = tempfile::tempdir().unwrap();
    let id = "com.example.event-counter";
    let mut manager = PluginManager::new(store.path().into()).unwrap();
    manager
        .install_local(&package(
            &examples.join(format!("event_counter{}", std::env::consts::EXE_SUFFIX)),
            id,
            "1.0.0",
        ))
        .unwrap();
    let actor = manager.spawn_actor(id).unwrap();
    actor.submit(1, PluginOperation::Enable(true)).unwrap();
    wait(&actor, 1);
    // The actor and transport queues hold at most 256 events each. Keep the
    // consumer paused until the negotiated retry path is exercised.
    for sequence in 2..602 {
        actor
            .submit(sequence, PluginOperation::Action("increment".into()))
            .unwrap();
        wait(&actor, sequence);
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut seen = std::collections::BTreeSet::new();
    while seen.len() < 600 {
        for bubble in actor.take_events(256) {
            assert!(seen.insert(bubble.id), "an event must be delivered once");
        }
        assert!(
            Instant::now() < deadline,
            "event delivery must resume after capacity is released"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(seen, (1..=600).map(|n| format!("count-{n}")).collect());
    let stats = actor.diagnostics();
    assert!(stats.requests >= 601 && stats.received_bytes > 0 && stats.sent_bytes > 0);
    assert_eq!(stats.timeouts, 0);
    actor.join().unwrap();
}

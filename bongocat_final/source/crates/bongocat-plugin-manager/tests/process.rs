//! The manager drives a real plugin binary discovered from a directory.
use bongocat_plugin_manager::{PluginManager, PluginState};
mod support;

#[test]
fn v1_frames_keep_their_delivered_wire_shape() {
    let fixtures: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/v1-requests.json")).unwrap();
    for fixture in fixtures {
        let request: bongocat_plugin_sdk::Request =
            serde_json::from_value(fixture.clone()).unwrap();
        let mut frame = Vec::new();
        bongocat_plugin_sdk::write_frame(&mut frame, &request).unwrap();
        assert_eq!(
            u32::from_le_bytes(frame[..4].try_into().unwrap()) as usize,
            frame.len() - 4
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&frame[4..]).unwrap(),
            fixture
        );
    }
}
fn install(directory: &std::path::Path) {
    let bytes = support::package(|_| {});
    let mut manager = PluginManager::new(directory.to_path_buf()).unwrap();
    manager
        .install(&bytes, &bongocat_plugin_manager::sha256_of(&bytes))
        .unwrap();
}

#[test]
fn discovery_lifecycle_config_and_ticks_run_a_real_process() {
    let directory = tempfile::tempdir().unwrap();
    install(directory.path());

    let mut manager = PluginManager::new(directory.path().to_path_buf()).unwrap();
    assert_eq!(manager.discover_plugins().unwrap(), 1);
    assert_eq!(manager.list_plugins()[0].1, PluginState::Discovered);

    manager.enable_plugin("com.example.host-fixture").unwrap();
    assert_eq!(manager.list_plugins()[0].1, PluginState::Enabled);
    let started = manager
        .plugin_output("com.example.host-fixture")
        .unwrap()
        .clone();
    assert!(started.model.nodes.iter().any(|node| matches!(
        node,
        bongocat_plugin_sdk::UiNode::Button { id, .. } if id == "toggle"
    )));

    assert!(
        manager
            .configure_plugin(
                "com.example.host-fixture",
                [(
                    "minutes".into(),
                    bongocat_plugin_sdk::ConfigValue::Integer(0)
                )]
                .into()
            )
            .is_err()
    );
    assert!(
        manager
            .action_plugin("com.example.host-fixture", "unknown")
            .is_err()
    );
    assert_eq!(manager.list_plugins()[0].1, PluginState::Enabled);

    manager
        .configure_plugin(
            "com.example.host-fixture",
            [(
                "minutes".into(),
                bongocat_plugin_sdk::ConfigValue::Integer(5),
            )]
            .into(),
        )
        .unwrap();
    manager.tick(250);
    manager
        .action_plugin("com.example.host-fixture", "restart")
        .unwrap();
    assert!(
        manager
            .plugin_output("com.example.host-fixture")
            .unwrap()
            .model
            .nodes
            .iter()
            .any(|node| matches!(node,
                bongocat_plugin_sdk::UiNode::Text { text } if text == "05:00"
            ))
    );
    manager.tick(1250);
    assert!(
        manager
            .plugin_output("com.example.host-fixture")
            .unwrap()
            .model
            .nodes
            .iter()
            .any(|node| matches!(node,
                bongocat_plugin_sdk::UiNode::Text { text } if text == "04:59"
            ))
    );
    manager.disable_plugin("com.example.host-fixture").unwrap();
    assert_eq!(manager.list_plugins()[0].1, PluginState::Disabled);
    assert!(
        manager
            .configure_plugin("com.example.host-fixture", Default::default())
            .is_err()
    );
    manager.shutdown().unwrap();
}

#[test]
fn persistence_failure_stops_uncommitted_configuration_and_can_be_retried() {
    let directory = tempfile::tempdir().unwrap();
    install(directory.path());
    let id = "com.example.host-fixture";
    let mut manager = PluginManager::new(directory.path().to_path_buf()).unwrap();
    manager.discover_plugins().unwrap();
    manager.enable_plugin(id).unwrap();
    let file = manager.data_root(id, false).unwrap().join("session.json");
    let saved = std::fs::read(&file).unwrap();
    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir(&file).unwrap();
    assert!(
        manager
            .configure_plugin(
                id,
                [(
                    "minutes".into(),
                    bongocat_plugin_sdk::ConfigValue::Integer(5)
                )]
                .into()
            )
            .is_err()
    );
    assert_eq!(manager.list_plugins()[0].1, PluginState::Failed);
    assert_eq!(
        manager.plugin_output(id).unwrap().config["minutes"],
        bongocat_plugin_sdk::ConfigValue::Integer(25)
    );
    assert!(manager.action_plugin(id, "toggle").is_err());
    std::fs::remove_dir(&file).unwrap();
    std::fs::write(&file, saved).unwrap();
    manager.enable_plugin(id).unwrap();
    assert_eq!(
        manager.plugin_output(id).unwrap().config["minutes"],
        bongocat_plugin_sdk::ConfigValue::Integer(25)
    );
    let saved = std::fs::read(&file).unwrap();
    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir(&file).unwrap();
    assert!(manager.configure_plugin(id, Default::default()).is_err());
    std::fs::remove_dir(&file).unwrap();
    std::fs::write(&file, saved).unwrap();
    // A failed entry still carries its saved enable request. Uninstall must
    // clear that request before retaining the session for a later reinstall.
    manager.uninstall(id).unwrap();
    install(directory.path());
    let mut reopened = PluginManager::new(directory.path().to_path_buf()).unwrap();
    reopened.discover_plugins().unwrap();
    reopened.resume_enabled().unwrap();
    assert_eq!(reopened.list_plugins()[0].1, PluginState::Discovered);
    reopened.shutdown().unwrap();
    manager.shutdown().unwrap();
}

#[test]
fn failed_start_does_not_mark_the_plugin_enabled_or_commit_output() {
    let directory = tempfile::tempdir().unwrap();
    install(directory.path());
    let id = "com.example.host-fixture";
    let mut manager = PluginManager::new(directory.path().to_path_buf()).unwrap();
    manager.discover_plugins().unwrap();
    let file = manager.data_root(id, true).unwrap().join("session.json");
    std::fs::create_dir(&file).unwrap();
    assert!(manager.enable_plugin(id).is_err());
    assert_eq!(manager.list_plugins()[0].1, PluginState::Failed);
    assert!(manager.plugin_output(id).unwrap().config.is_empty());
    assert!(manager.action_plugin(id, "toggle").is_err());
    std::fs::remove_dir(&file).unwrap();
    manager.enable_plugin(id).unwrap();
    assert_eq!(manager.list_plugins()[0].1, PluginState::Enabled);
    manager.shutdown().unwrap();
}

#[test]
fn a_process_that_refuses_to_read_is_killed_without_blocking_its_peer() {
    use bongocat_plugin_manager::PluginOperation;
    use std::time::{Duration, Instant};
    let store = tempfile::tempdir().unwrap();
    let mut manager = PluginManager::new(store.path().into()).unwrap();
    for (id, description) in [
        ("com.example.blocked-writer", "refuse-read"),
        ("com.example.healthy-writer", "normal"),
    ] {
        manager
            .install_local(&support::package(|manifest| {
                manifest["metadata"]["id"] = id.into();
                manifest["metadata"]["description"] = description.into();
            }))
            .unwrap();
    }
    let session = manager
        .data_root("com.example.blocked-writer", true)
        .unwrap()
        .join("session.json");
    std::fs::write(session, serde_json::to_vec(&serde_json::json!({"schema_version":1,"config":{},"state":vec![0u8;64*1024],"enabled":false})).unwrap()).unwrap();
    drop(manager);
    let mut manager = PluginManager::new(store.path().into()).unwrap();
    manager.discover_plugins().unwrap();
    manager.enable_plugin("com.example.healthy-writer").unwrap();
    let slow = manager.spawn_actor("com.example.blocked-writer").unwrap();
    let healthy = manager.spawn_actor("com.example.healthy-writer").unwrap();
    slow.submit(1, PluginOperation::Enable(true)).unwrap();
    healthy
        .submit(2, PluginOperation::Action("benchmark-noop".into()))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if let Ok(completion) = healthy.try_completion() {
            completion.result.unwrap();
            break;
        }
        assert!(
            Instant::now() < deadline,
            "peer must not wait for a blocked pipe write"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let deadline = Instant::now() + Duration::from_secs(7);
    loop {
        if let Ok(completion) = slow.try_completion() {
            assert!(completion.result.is_err());
            break;
        }
        assert!(
            Instant::now() < deadline,
            "blocked writer must reach its start deadline"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(slow.diagnostics().timeouts, 1);
    slow.join().unwrap();
    healthy.join().unwrap();
}

#[test]
fn an_out_of_order_reply_fails_the_real_process_without_losing_enable_intent() {
    let store = tempfile::tempdir().unwrap();
    let mut manager = PluginManager::new(store.path().into()).unwrap();
    manager
        .install_local(&support::package(|manifest| {
            manifest["metadata"]["description"] = "out-of-order".into();
        }))
        .unwrap();
    manager.enable_plugin(support::ID).unwrap();
    assert!(
        manager
            .action_plugin(support::ID, "benchmark-noop")
            .is_err()
    );
    assert_eq!(manager.list_plugins()[0].1, PluginState::Failed);
    let session: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            manager
                .data_root(support::ID, true)
                .unwrap()
                .join("session.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(session["enabled"], true);
    manager.shutdown().unwrap();
}

#[test]
fn continuous_pushes_reach_the_actor_and_do_not_consume_reply_sequences() {
    use bongocat_plugin_manager::PluginOperation;
    use std::time::{Duration, Instant};
    let store = tempfile::tempdir().unwrap();
    let mut manager = PluginManager::new(store.path().into()).unwrap();
    manager
        .install_local(&support::package(|manifest| {
            manifest["metadata"]["description"] = "push".into();
            manifest["metadata"]["capabilities"]
                .as_array_mut()
                .unwrap()
                .push("push_output_v1".into());
        }))
        .unwrap();
    manager.enable_plugin(support::ID).unwrap();
    let actor = manager.spawn_actor(support::ID).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(Some(projection)) = actor.take_projection()
            && projection.output.model.nodes.iter().any(|node| {
                matches!(node,
                    bongocat_plugin_sdk::UiNode::Text { text } if text.starts_with("push ")
                )
            })
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "unsolicited pushes must wake the actor"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    for sequence in 1..=5 {
        actor
            .submit(sequence, PluginOperation::Action("benchmark-noop".into()))
            .unwrap();
        loop {
            if let Ok(completion) = actor.try_completion() {
                assert_eq!(completion.sequence, sequence);
                completion.result.unwrap();
                break;
            }
            assert!(Instant::now() < deadline, "pushes must not starve replies");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    actor.join().unwrap();
}

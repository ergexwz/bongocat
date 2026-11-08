//! Deterministic process fixture. Built by workspace tests, excluded from product packaging.
use bongocat_plugin_sdk::{
    ConfigValue, Configuration, HostMessage, Plugin, PluginOutput, PluginView, UiNode,
};

struct Fixture {
    id: &'static str,
    config: Configuration,
    remaining: u64,
    last_tick: Option<u64>,
}

impl Fixture {
    fn output(&self) -> PluginOutput {
        let minutes = match self.config.get("minutes") {
            Some(ConfigValue::Integer(value)) => *value,
            _ => 25,
        };
        PluginOutput {
            config: self.config.clone(),
            model: PluginView {
                nodes: vec![
                    UiNode::Text {
                        text: format!(
                            "{:02}:{:02}",
                            self.remaining / 60_000,
                            self.remaining / 1000 % 60
                        ),
                    },
                    UiNode::button("toggle", "Start", bongocat_plugin_sdk::ActionIcon::Play),
                    UiNode::button(
                        "restart",
                        "Restart",
                        bongocat_plugin_sdk::ActionIcon::Restart,
                    ),
                ],
                ..Default::default()
            },
            settings: PluginView {
                nodes: vec![UiNode::Integer {
                    key: "minutes".into(),
                    label: "Minutes".into(),
                    value: minutes,
                    min: 1,
                    max: 120,
                }],
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

impl Plugin for Fixture {
    fn id(&self) -> &'static str {
        self.id
    }

    fn handle(&mut self, message: HostMessage) -> Result<PluginOutput, String> {
        match message {
            HostMessage::Start { config, .. } | HostMessage::Configure { config } => {
                let mut config = config;
                config
                    .entry("minutes".into())
                    .or_insert(ConfigValue::Integer(25));
                if !matches!(config.get("minutes"), Some(ConfigValue::Integer(1..=120))) {
                    return Err("invalid minutes".into());
                }
                self.config = config;
            }
            HostMessage::Tick { elapsed_ms } => {
                if let Some(last) = self.last_tick {
                    self.remaining = self
                        .remaining
                        .saturating_sub(elapsed_ms.saturating_sub(last));
                }
                self.last_tick = Some(elapsed_ms);
            }
            HostMessage::Action { id } => match id.as_str() {
                "slow" => std::thread::sleep(std::time::Duration::from_secs(5)),
                "crash" => std::process::exit(1),
                "benchmark-noop" => {}
                "toggle" | "restart" => {
                    if let Some(ConfigValue::Integer(minutes)) = self.config.get("minutes") {
                        self.remaining = *minutes as u64 * 60_000;
                    }
                }
                _ => return Err("unknown action".into()),
            },
            _ => {}
        }
        Ok(self.output())
    }
}

fn wire_fixture(mut fixture: Fixture, mode: &str) -> std::io::Result<()> {
    use bongocat_plugin_sdk::{Request, Response, read_frame, write_frame};
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };
    let output = Arc::new(Mutex::new(std::io::stdout()));
    let stop = Arc::new(AtomicBool::new(false));
    let pushing = Arc::new(AtomicBool::new(false));
    let producer = if mode == "push" {
        let output = Arc::clone(&output);
        let stop = Arc::clone(&stop);
        let pushing = Arc::clone(&pushing);
        Some(std::thread::spawn(move || -> std::io::Result<()> {
            let mut revision = 0u64;
            while !stop.load(Ordering::Acquire) {
                if pushing.load(Ordering::Acquire) {
                    revision += 1;
                    write_frame(
                        &mut *output.lock().unwrap(),
                        &Response {
                            sequence: 0,
                            result: Ok(PluginOutput {
                                model: PluginView {
                                    nodes: vec![UiNode::Text {
                                        text: format!("push {revision}"),
                                    }],
                                    ..Default::default()
                                },
                                ..Default::default()
                            }),
                        },
                    )?;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Ok(())
        }))
    } else {
        None
    };
    let result = (|| {
        let mut input = std::io::stdin().lock();
        let mut ready = false;
        loop {
            let request: Request = read_frame(&mut input)?;
            let stopped = matches!(request.message, HostMessage::Stop);
            let reorder = mode == "out-of-order"
                && matches!(&request.message, HostMessage::Action { id } if id == "benchmark-noop");
            let result = match request.message {
                HostMessage::Handshake {
                    protocol_version: 1,
                    id,
                } if !ready && id == fixture.id => {
                    ready = true;
                    Ok(PluginOutput::default())
                }
                message if ready => fixture.handle(message),
                _ => Err("handshake required".into()),
            };
            write_frame(
                &mut *output.lock().unwrap(),
                &Response {
                    sequence: request.sequence + u64::from(reorder),
                    result,
                },
            )?;
            pushing.store(ready && !stopped, Ordering::Release);
            if stopped {
                return Ok(());
            }
        }
    })();
    stop.store(true, Ordering::Release);
    if let Some(producer) = producer {
        producer.join().unwrap()?;
    }
    result
}

fn main() -> std::io::Result<()> {
    if std::path::Path::new("fail-start").exists() {
        std::process::exit(1);
    }
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read("plugin.json")?)?;
    if manifest["metadata"]["description"] == "refuse-read" {
        let request: bongocat_plugin_sdk::Request =
            bongocat_plugin_sdk::read_frame(&mut std::io::stdin().lock())?;
        bongocat_plugin_sdk::write_frame(
            &mut std::io::stdout().lock(),
            &bongocat_plugin_sdk::Response {
                sequence: request.sequence,
                result: Ok(PluginOutput::default()),
            },
        )?;
        std::thread::sleep(std::time::Duration::from_secs(30));
        return Ok(());
    }
    let id = manifest["metadata"]["id"]
        .as_str()
        .expect("fixture id")
        .to_owned()
        .leak();
    let mut fixture = Fixture {
        id,
        config: Configuration::new(),
        remaining: 0,
        last_tick: None,
    };
    let mode = manifest["metadata"]["description"]
        .as_str()
        .unwrap_or_default();
    if matches!(mode, "push" | "out-of-order") {
        wire_fixture(fixture, mode)
    } else {
        bongocat_plugin_sdk::run(&mut fixture)
    }
}

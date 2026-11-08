//! Event-driven counter: checkpoint migration and acknowledged notifications.
use bongocat_plugin_sdk::{
    BubbleKind, HostMessage, PluginBubble, PluginOutput, PluginView, UiNode, v2,
};

#[derive(Default)]
struct Counter {
    value: u64,
}
impl bongocat_plugin_sdk::Plugin for Counter {
    fn id(&self) -> &'static str {
        "com.example.event-counter"
    }
    fn handle(&mut self, message: HostMessage) -> Result<PluginOutput, String> {
        let mut bubbles = Vec::new();
        match message {
            HostMessage::Restore {
                state,
                schema_version,
                ..
            } => {
                if schema_version > 1 {
                    return Err("unsupported checkpoint version".into());
                }
                self.value = if state.is_empty() {
                    0
                } else {
                    u64::from_le_bytes(state.try_into().map_err(|_| "invalid counter state")?)
                };
            }
            HostMessage::Action { id } if id == "increment" => {
                self.value = self.value.saturating_add(1);
                bubbles.push(PluginBubble {
                    id: format!("count-{}", self.value),
                    kind: BubbleKind::Todo,
                    source: "Counter".into(),
                    title: "Counter updated".into(),
                    subtitle: self.value.to_string(),
                    urgent: false,
                    action: "increment".into(),
                    auto_dismiss_ms: Some(1000),
                });
            }
            HostMessage::Tick { .. } => return Err("this plugin never requests a tick".into()),
            _ => {}
        }
        Ok(PluginOutput {
            model: PluginView {
                nodes: vec![UiNode::Text {
                    text: self.value.to_string(),
                }],
                ..Default::default()
            },
            state: self.value.to_le_bytes().to_vec(),
            state_schema_version: 1,
            bubbles,
            ..Default::default()
        })
    }
}
impl v2::Plugin for Counter {}
fn main() -> std::io::Result<()> {
    v2::run(&mut Counter::default())
}

//! Three half-second ticks, followed by no scheduled work.
use bongocat_plugin_sdk::{HostMessage, PluginOutput, PluginView, UiNode, v2};
struct Timer {
    remaining: u8,
}
impl bongocat_plugin_sdk::Plugin for Timer {
    fn id(&self) -> &'static str {
        "com.example.scheduled-timer"
    }
    fn handle(&mut self, message: HostMessage) -> Result<PluginOutput, String> {
        match message {
            HostMessage::Restore { schema_version, .. } if schema_version > 1 => {
                return Err("unsupported checkpoint version".into());
            }
            HostMessage::Tick { .. } => self.remaining = self.remaining.saturating_sub(1),
            HostMessage::Action { id } if id == "restart" => self.remaining = 3,
            _ => {}
        }
        Ok(PluginOutput {
            model: PluginView {
                nodes: vec![UiNode::Text {
                    text: self.remaining.to_string(),
                }],
                ..Default::default()
            },
            state_schema_version: 1,
            ..Default::default()
        })
    }
}
impl v2::Plugin for Timer {
    fn next_wake_ms(&self) -> Option<u64> {
        (self.remaining > 0).then_some(500)
    }
}
fn main() -> std::io::Result<()> {
    v2::run(&mut Timer { remaining: 3 })
}

//! Pet context and a host-rendered HUD, without platform or UI framework types.
use bongocat_plugin_sdk::{
    BubbleKind, HostMessage, PluginBubble, PluginOutput, PluginView, UiNode, v2,
};
struct Hud {
    visible: bool,
    notices: u64,
}
impl bongocat_plugin_sdk::Plugin for Hud {
    fn id(&self) -> &'static str {
        "com.example.companion-hud"
    }
    fn handle(&mut self, message: HostMessage) -> Result<PluginOutput, String> {
        let mut bubbles = Vec::new();
        match message {
            HostMessage::Restore { schema_version, .. } if schema_version > 1 => {
                return Err("unsupported checkpoint version".into());
            }
            HostMessage::PetState { state } => {
                if state.visible && !self.visible {
                    self.notices += 1;
                    bubbles.push(PluginBubble {
                        id: format!("visible-{}", self.notices),
                        kind: BubbleKind::Todo,
                        source: "HUD example".into(),
                        title: "Pet visible".into(),
                        subtitle: "Companion context updated".into(),
                        urgent: false,
                        action: "acknowledge".into(),
                        auto_dismiss_ms: Some(1500),
                    });
                }
                self.visible = state.visible;
            }
            _ => {}
        }
        Ok(PluginOutput {
            model: PluginView {
                nodes: vec![UiNode::Text {
                    text: if self.visible {
                        "Pet visible"
                    } else {
                        "Pet hidden"
                    }
                    .into(),
                }],
                ..Default::default()
            },
            state_schema_version: 1,
            bubbles,
            ..Default::default()
        })
    }
}
impl v2::Plugin for Hud {
    fn subscriptions(&self) -> v2::Subscriptions {
        v2::Subscriptions {
            pet_state: true,
            ..Default::default()
        }
    }
}
fn main() -> std::io::Result<()> {
    v2::run(&mut Hud {
        visible: true,
        notices: 0,
    })
}

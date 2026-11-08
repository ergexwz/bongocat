//! Protocol-neutral process fixture for isolated 0/3/10-plugin benchmarks.
use bongocat_plugin_sdk::{HostMessage, PluginMetadata, PluginOutput, v2};
use serde::Deserialize;
#[derive(Deserialize)]
struct Manifest {
    metadata: PluginMetadata,
}
struct Fixture {
    id: &'static str,
}
impl bongocat_plugin_sdk::Plugin for Fixture {
    fn id(&self) -> &'static str {
        self.id
    }
    fn handle(&mut self, _message: HostMessage) -> Result<PluginOutput, String> {
        Ok(PluginOutput::default())
    }
}
impl v2::Plugin for Fixture {}
fn main() -> std::io::Result<()> {
    let manifest: Manifest = serde_json::from_slice(&std::fs::read("plugin.json")?)?;
    let mut plugin = Fixture {
        id: manifest.metadata.id.leak(),
    };
    if manifest.metadata.protocol_version == 1 {
        bongocat_plugin_sdk::run(&mut plugin)
    } else {
        v2::run(&mut plugin)
    }
}

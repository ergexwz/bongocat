//! Map the runtime's privacy-filtered edges into the process protocol.
use bongocat_plugin_service::{InputEventControl, PluginInputEvent};
use bongocat_runtime::{ActivityControl, ActivitySubscription, InputEdge, RuntimeClient};

pub(crate) struct PluginActivity(pub(crate) ActivitySubscription);

/// The same counts the runtime already exposes, named for the plugin protocol.
///
/// A plugin sees absolute device totals and the bounded special-key subsets,
/// never a key identity, text or pointer position. The counter plugin only
/// reads the two totals, but the complete shape is required by the message
/// contract.
pub(crate) fn activity(client: &RuntimeClient) -> bongocat_plugin_service::InputActivity {
    let diagnostics = client.input_diagnostics();
    bongocat_plugin_service::InputActivity {
        keyboard_down: diagnostics.keyboard_activity,
        keyboard_up: diagnostics.keyboard_release,
        mouse_down: diagnostics.mouse_activity,
        mouse_up: diagnostics.mouse_release,
        space_down: diagnostics.space_activity,
        space_up: diagnostics.space_release,
        enter_down: diagnostics.enter_activity,
        enter_up: diagnostics.enter_release,
        backspace_down: diagnostics.backspace_activity,
        backspace_up: diagnostics.backspace_release,
        tab_down: diagnostics.tab_activity,
        tab_up: diagnostics.tab_release,
        shift_left_down: diagnostics.shift_left_activity,
        shift_left_up: diagnostics.shift_left_release,
        shift_right_down: diagnostics.shift_right_activity,
        shift_right_up: diagnostics.shift_right_release,
        ctrl_left_down: diagnostics.ctrl_left_activity,
        ctrl_left_up: diagnostics.ctrl_left_release,
    }
}

impl bongocat_plugin_service::PluginInputEvents for PluginActivity {
    fn set_waker(&self, thread: std::thread::Thread) {
        self.0.set_waker(thread);
    }

    fn drain(&mut self) -> (Vec<PluginInputEvent>, u64) {
        let (events, overflow) = self.0.drain();
        let events = events
            .into_iter()
            .map(|event| PluginInputEvent {
                control: match event.control {
                    ActivityControl::Keyboard => InputEventControl::Keyboard,
                    ActivityControl::Space => InputEventControl::Space,
                    ActivityControl::Enter => InputEventControl::Enter,
                    ActivityControl::Backspace => InputEventControl::Backspace,
                    ActivityControl::Tab => InputEventControl::Tab,
                    ActivityControl::ShiftLeft => InputEventControl::ShiftLeft,
                    ActivityControl::ShiftRight => InputEventControl::ShiftRight,
                    ActivityControl::CtrlLeft => InputEventControl::CtrlLeft,
                    ActivityControl::MouseLeft => InputEventControl::MouseLeft,
                    ActivityControl::MouseRight => InputEventControl::MouseRight,
                },
                pressed: event.edge == InputEdge::Down,
                captured_ms: event.at.value(),
            })
            .collect();
        (events, overflow)
    }
}

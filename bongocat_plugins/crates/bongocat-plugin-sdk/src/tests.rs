use super::*;

fn output() -> PluginOutput {
    PluginOutput::default()
}

#[test]
fn companion_menu_is_bounded_and_unique() {
    let mut output = output();
    output.menu = vec![PluginMenuItem {
        id: "open".into(),
        label: "Open".into(),
    }];
    assert!(output.validate());

    output.menu.push(PluginMenuItem {
        id: "open".into(),
        label: "Again".into(),
    });
    assert!(!output.validate());

    output.menu = (0..PLUGIN_MENU_LIMIT + 1)
        .map(|index| PluginMenuItem {
            id: format!("item-{index}"),
            label: "Item".into(),
        })
        .collect();
    assert!(!output.validate());
}

#[test]
fn bubbles_reject_control_text_and_unknown_action_icons() {
    let mut output = output();
    output.bubbles = vec![PluginBubble {
        id: "bubble-1".into(),
        kind: BubbleKind::Todo,
        source: "Todo due".into(),
        title: "Task".into(),
        subtitle: "is due now".into(),
        urgent: true,
        action: "open-todo".into(),
        auto_dismiss_ms: None,
    }];
    assert!(output.validate());

    output.bubbles[0].title = "bad\nline".into();
    assert!(!output.validate());
    assert_eq!(ActionIcon::Settings.as_str(), "settings");
}

#[test]
fn pet_bounds_reject_degenerate_rectangles() {
    assert!(
        PetBounds {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
            scale: 1.0,
        }
        .validate()
    );
    assert!(
        !PetBounds {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 10.0,
            scale: 1.0,
        }
        .validate()
    );
}

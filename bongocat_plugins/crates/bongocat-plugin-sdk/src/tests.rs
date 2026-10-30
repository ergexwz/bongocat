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

#[test]
fn pet_state_rejects_an_unusable_model_id() {
    assert!(
        PetState {
            model: Some(ModelInfo {
                id: "gamepad".into(),
                built_in: true,
            }),
            visible: true,
        }
        .validate()
    );
    assert!(
        !PetState {
            model: Some(ModelInfo {
                id: "bad\nid".into(),
                built_in: true,
            }),
            visible: true,
        }
        .validate()
    );
    assert!(
        PetState {
            model: None,
            visible: false,
        }
        .validate()
    );
}

#[test]
fn menu_tree_is_bounded_and_checked_only_on_leaves() {
    let mut output = output();
    output.menu_tree = vec![PluginMenuNode {
        id: "scale".into(),
        label: "Scale".into(),
        checked: None,
        children: (0..PLUGIN_MENU_LIMIT)
            .map(|index| PluginMenuNode {
                id: format!("scale-{index}"),
                label: format!("{}%", (index + 1) * 20),
                checked: Some(index == 4),
                children: Vec::new(),
            })
            .collect(),
    }];
    assert!(output.validate());

    // A submenu cannot carry a check mark of its own.
    output.menu_tree[0].checked = Some(true);
    assert!(!output.validate());

    // Duplicate sibling ids are a host hit-testing bug, not a preference.
    output.menu_tree[0].checked = None;
    output.menu_tree[0].children[1].id = output.menu_tree[0].children[0].id.clone();
    assert!(!output.validate());

    // Depth two is the contract; a grandchild is refused.
    output.menu_tree[0].children[0].children = vec![PluginMenuNode {
        id: "nested".into(),
        label: "Nested".into(),
        checked: None,
        children: Vec::new(),
    }];
    assert!(!output.validate());
}

#[test]
fn menu_tree_rejects_more_than_the_top_level_limit() {
    let mut output = output();
    output.menu_tree = (0..PLUGIN_MENU_LIMIT + 1)
        .map(|index| PluginMenuNode {
            id: format!("item-{index}"),
            label: "Item".into(),
            checked: None,
            children: Vec::new(),
        })
        .collect();
    assert!(!output.validate());
}

#[test]
fn menu_tree_serialization_omits_absent_optional_fields() {
    let node = PluginMenuNode {
        id: "open".into(),
        label: "Open".into(),
        checked: None,
        children: Vec::new(),
    };
    assert_eq!(
        serde_json::to_string(&node).unwrap(),
        r#"{"id":"open","label":"Open"}"#
    );
}

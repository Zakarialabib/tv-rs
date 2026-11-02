//! Rule, scene, and command grammar AST templates (Tier C Knowledge Base).
//!
//! Provides grounded AST schema examples and declarative few-shot templates
//! from `tv-rs-rules` (`Condition`, `Action`, `Rule`, `Scene`, `Schedule`) as well
//! as hardware control payloads (e.g. video wall, key lock, input routing).

use serde::{Deserialize, Serialize};

/// A grounding template representing a valid AST schema, rule structure, or command payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrammarTemplate {
    /// Unique template identifier (e.g. `videowall_2x2_scene`, `keypad_lock_action`).
    pub id: String,
    /// Domain category (e.g. `scene`, `rule`, `action`, `command`, `schedule`).
    pub category: String,
    /// Target capability or protocol (e.g. `video_wall`, `input_select`, `sicp`, `mdc`).
    pub target: String,
    /// Short human-readable title.
    pub title: String,
    /// Description of what this template achieves and its required parameters.
    pub description: String,
    /// Formatted JSON AST representation.
    pub json_schema_payload: String,
    /// Associated keywords and tokens for lexical indexing.
    pub tags: Vec<String>,
}

/// Registry of AST grammar templates.
#[derive(Debug, Clone, Default)]
pub struct TemplateRegistry {
    templates: Vec<GrammarTemplate>,
}

impl TemplateRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            templates: Vec::new(),
        }
    }

    /// Adds a template to the registry.
    pub fn add(&mut self, template: GrammarTemplate) {
        self.templates.push(template);
    }

    /// Returns all registered grammar templates.
    #[must_use]
    pub fn templates(&self) -> &[GrammarTemplate] {
        &self.templates
    }

    /// Populates canonical AST templates for rules, scenes, and video wall controls.
    pub fn populate_defaults(&mut self) {
        // 1. Video Wall Scene Template
        self.add(GrammarTemplate {
            id: "videowall_2x2_scene".to_string(),
            category: "scene".to_string(),
            target: "video_wall".to_string(),
            title: "2x2 Video Wall Setup Scene".to_string(),
            description:
                "A Scene grouping 4 displays into a 2x2 matrix video wall with tile coordinates."
                    .to_string(),
            json_schema_payload: serde_json::json!({
                "id": "videowall_2x2_lobby",
                "name": "Lobby 2x2 Video Wall",
                "actions": [
                    {
                        "action_type": "custom",
                        "name": "video_wall_tile",
                        "target": "display_top_left",
                        "payload": { "matrix_h": 2, "matrix_v": 2, "tile_h": 1, "tile_v": 1 }
                    },
                    {
                        "action_type": "custom",
                        "name": "video_wall_tile",
                        "target": "display_top_right",
                        "payload": { "matrix_h": 2, "matrix_v": 2, "tile_h": 2, "tile_v": 1 }
                    },
                    {
                        "action_type": "custom",
                        "name": "video_wall_tile",
                        "target": "display_bottom_left",
                        "payload": { "matrix_h": 2, "matrix_v": 2, "tile_h": 1, "tile_v": 2 }
                    },
                    {
                        "action_type": "custom",
                        "name": "video_wall_tile",
                        "target": "display_bottom_right",
                        "payload": { "matrix_h": 2, "matrix_v": 2, "tile_h": 2, "tile_v": 2 }
                    }
                ]
            })
            .to_string(),
            tags: vec![
                "videowall".into(),
                "2x2".into(),
                "scene".into(),
                "matrix".into(),
                "tile".into(),
            ],
        });

        // 2. Physical Button & Keypad Lock Action
        self.add(GrammarTemplate {
            id: "keypad_lock_action".to_string(),
            category: "action".to_string(),
            target: "keypad_lock".to_string(),
            title: "Lock Physical Hardware Buttons".to_string(),
            description:
                "Locks the physical chassis buttons or remote IR sensor on commercial displays."
                    .to_string(),
            json_schema_payload: serde_json::json!({
                "action_type": "custom",
                "name": "set_keypad_lock",
                "target": "lobby_display_group",
                "payload": { "locked": true, "block_ir": false }
            })
            .to_string(),
            tags: vec![
                "keypad".into(),
                "lock".into(),
                "buttons".into(),
                "action".into(),
            ],
        });

        // 3. Declarative Rule Engine AST Template
        self.add(GrammarTemplate {
            id: "signage_overheat_rule".to_string(),
            category: "rule".to_string(),
            target: "telemetry".to_string(),
            title: "Overheating Protection Rule Template".to_string(),
            description: "Automated rule triggering backlight dimming or standby when temperature threshold exceeded.".to_string(),
            json_schema_payload: serde_json::json!({
                "id": "rule_overheat_protection",
                "name": "Protect Hot Display",
                "priority": "critical",
                "conditions": [
                    {
                        "condition_type": "telemetry",
                        "field": "temperature_celsius",
                        "operator": "greater_than",
                        "value": 75
                    }
                ],
                "actions": [
                    {
                        "action_type": "notify",
                        "recipient": "operations_manager",
                        "message": "Display temperature exceeded 75C, dimming backlight",
                        "level": "warning"
                    },
                    {
                        "action_type": "custom",
                        "name": "dim_backlight",
                        "target": "display_01",
                        "payload": { "to_percent": 20 }
                    }
                ]
            }).to_string(),
            tags: vec!["rule".into(), "overheat".into(), "telemetry".into(), "temperature".into(), "condition".into()],
        });

        // 4. Input Routing Action
        self.add(GrammarTemplate {
            id: "switch_source_action".to_string(),
            category: "action".to_string(),
            target: "input_select".to_string(),
            title: "Switch Source to HDMI / Signage Master".to_string(),
            description: "Switches the active input source of a display to a specified HDMI port or digital signage channel.".to_string(),
            json_schema_payload: serde_json::json!({
                "action_type": "switch_source",
                "target": "entrance_display",
                "source": "HDMI1"
            }).to_string(),
            tags: vec!["input".into(), "source".into(), "hdmi".into(), "switch".into()],
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_populate_valid_json_templates() {
        let mut registry = TemplateRegistry::new();
        registry.populate_defaults();
        assert!(!registry.templates().is_empty());

        for template in registry.templates() {
            let parsed: Result<serde_json::Value, _> =
                serde_json::from_str(&template.json_schema_payload);
            assert!(
                parsed.is_ok(),
                "Template {} must contain valid JSON",
                template.id
            );
        }
    }
}

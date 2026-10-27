//! Integration tests for `tv-rs-rules`.

use std::collections::HashMap;
use std::time::{Duration, Instant};
use tv_rs_rules::*;
use tv_rs_vision::VisionEvent;

#[test]
fn test_conceptual_rule_queue_alert() {
    // Conceptual rule:
    // If entrance camera detects more than 3 people waiting, and time is between 12:00 and 14:00,
    // show queue message on entrance TV and notify manager.
    let rule = Rule::new(
        "entrance_queue_alert",
        "Entrance Queue Alert",
        Condition::and(vec![
            Condition::Vision(VisionCondition::PersonCount {
                zone: "entrance".to_string(),
                op: ComparisonOp::Greater,
                count: 3,
            }),
            Condition::Schedule(Schedule::time_range("12:00", "14:00").unwrap()),
        ]),
        vec![
            Action::ShowMessage {
                target: "entrance_tv".to_string(),
                message: "High queue volume at entrance. Please proceed to checkout B.".to_string(),
                duration_secs: Some(30),
            },
            Action::Notify {
                recipient: "manager".to_string(),
                message: "Queue volume exceeded 3 people at entrance camera.".to_string(),
                level: NotificationLevel::Warning,
            },
        ],
    )
    .with_priority(Priority::High);

    let mut engine = RuleEngine::new();
    engine.add_rule(rule);

    let event = RuleEvent::from(VisionEvent::PersonDetected {
        zone: "entrance".to_string(),
        count: 4,
        timestamp: None,
    });

    // 1. Within schedule (12:30 PM) -> Triggers both actions
    let time_in_range = TimeOfDay::parse("12:30").unwrap();
    let actions = engine.process_event_at(&event, time_in_range, Instant::now());
    assert_eq!(actions.len(), 2);
    assert!(matches!(
        actions[0],
        Action::ShowMessage { .. } | Action::Notify { .. }
    ));

    // 2. Outside schedule (11:30 AM) -> Does NOT trigger
    let time_out_of_range = TimeOfDay::parse("11:30").unwrap();
    let actions_out = engine.process_event_at(&event, time_out_of_range, Instant::now());
    assert!(actions_out.is_empty());

    // 3. Count threshold not met (2 people) at 12:30 PM -> Does NOT trigger
    let event_low_count = RuleEvent::from(VisionEvent::PersonDetected {
        zone: "entrance".to_string(),
        count: 2,
        timestamp: None,
    });
    let actions_low = engine.process_event_at(&event_low_count, time_in_range, Instant::now());
    assert!(actions_low.is_empty());
}

#[test]
fn test_cooldown_tracking() {
    let rule = Rule::new(
        "motion_alert",
        "Motion Alert",
        Condition::Always,
        vec![Action::ShowMessage {
            target: "screen_1".to_string(),
            message: "Motion detected".to_string(),
            duration_secs: None,
        }],
    )
    .with_cooldown_secs(10);

    let mut engine = RuleEngine::new();
    engine.add_rule(rule);

    let event = RuleEvent::Custom {
        name: "test".to_string(),
        payload: Default::default(),
    };

    let start = Instant::now();
    let time = TimeOfDay::new(10, 0, 0);

    // First trigger -> Actions returned
    let actions1 = engine.process_event_at(&event, time, start);
    assert_eq!(actions1.len(), 1);

    // Second trigger after 5 seconds (within 10s cooldown) -> Blocked
    let actions2 = engine.process_event_at(&event, time, start + Duration::from_secs(5));
    assert!(actions2.is_empty());

    // Third trigger after 11 seconds (cooldown expired) -> Triggers again
    let actions3 = engine.process_event_at(&event, time, start + Duration::from_secs(11));
    assert_eq!(actions3.len(), 1);
}

#[test]
fn test_priority_conflict_resolution() {
    // Low priority rule targeting entrance_tv
    let rule_low = Rule::new(
        "rule_low",
        "Low Priority Promo",
        Condition::Always,
        vec![Action::ShowMessage {
            target: "entrance_tv".to_string(),
            message: "Welcome offer!".to_string(),
            duration_secs: None,
        }],
    )
    .with_priority(Priority::Low);

    // Emergency priority rule targeting same entrance_tv
    let rule_emergency = Rule::new(
        "rule_emergency",
        "Emergency Broadcast",
        Condition::Always,
        vec![Action::ShowMessage {
            target: "entrance_tv".to_string(),
            message: "Emergency Evacuation".to_string(),
            duration_secs: None,
        }],
    )
    .with_priority(Priority::Emergency);

    let mut engine = RuleEngine::new();
    engine.add_rule(rule_low);
    engine.add_rule(rule_emergency);

    let event = RuleEvent::Custom {
        name: "tick".to_string(),
        payload: Default::default(),
    };

    let actions = engine.process_event(&event);
    assert_eq!(actions.len(), 1);
    if let Action::ShowMessage { message, .. } = &actions[0] {
        assert_eq!(message, "Emergency Evacuation");
    } else {
        panic!("Expected ShowMessage action");
    }
}

#[test]
fn test_scene_activation() {
    let mut registry = SceneRegistry::new();
    let scene = Scene::new(
        "lunch_break",
        "Lunch Break Scene",
        vec![
            Action::PowerControl {
                target: "side_tv".to_string(),
                power_on: true,
            },
            Action::SwitchSource {
                target: "side_tv".to_string(),
                source: "HDMI1".to_string(),
            },
        ],
    );
    registry.register(scene);

    let rule = Rule::new(
        "activate_lunch",
        "Activate Lunch Scene",
        Condition::Always,
        vec![Action::ActivateScene {
            scene_id: "lunch_break".to_string(),
        }],
    );

    let mut engine = RuleEngine::new();
    if let Some(s) = registry.get("lunch_break") {
        engine.register_scene(s.clone());
    }
    engine.add_rule(rule);

    let event = RuleEvent::Custom {
        name: "lunch_time".to_string(),
        payload: Default::default(),
    };

    let actions = engine.process_event(&event);
    assert_eq!(actions.len(), 2);
}

#[test]
fn test_schedule_overnight() {
    let schedule = Schedule::time_range("22:00", "06:00").unwrap();

    let t_night = TimeOfDay::parse("23:30").unwrap();
    let t_morning = TimeOfDay::parse("04:15").unwrap();
    let t_day = TimeOfDay::parse("14:00").unwrap();

    assert!(schedule.is_active(&t_night, None));
    assert!(schedule.is_active(&t_morning, None));
    assert!(!schedule.is_active(&t_day, None));
}

#[test]
fn test_property_conditions_and_logic() {
    let mut engine = RuleEngine::new();
    engine.set_state("store_open", Value::Bool(true));
    engine.set_state("occupancy_level", Value::Integer(25));

    let rule = Rule::new(
        "open_high_occupancy",
        "Store open and high occupancy",
        Condition::and(vec![
            Condition::Property {
                key: "store_open".to_string(),
                op: ComparisonOp::Equal,
                value: Value::Bool(true),
            },
            Condition::or(vec![
                Condition::Property {
                    key: "occupancy_level".to_string(),
                    op: ComparisonOp::GreaterEq,
                    value: Value::Integer(20),
                },
                Condition::Property {
                    key: "vip_guest".to_string(),
                    op: ComparisonOp::Equal,
                    value: Value::Bool(true),
                },
            ]),
        ]),
        vec![Action::ShowMessage {
            target: "main_screen".to_string(),
            message: "Welcome valued customers".to_string(),
            duration_secs: None,
        }],
    );

    engine.add_rule(rule);

    let event = RuleEvent::Custom {
        name: "state_check".to_string(),
        payload: HashMap::new(),
    };

    let actions = engine.process_event(&event);
    assert_eq!(actions.len(), 1);
}

#[test]
fn test_rule_json_serialization_roundtrip() {
    let rule = Rule::new(
        "rule_1",
        "Test Rule",
        Condition::Vision(VisionCondition::PersonCount {
            zone: "entrance".to_string(),
            op: ComparisonOp::Greater,
            count: 3,
        }),
        vec![Action::ShowMessage {
            target: "tv_1".to_string(),
            message: "Hello".to_string(),
            duration_secs: Some(10),
        }],
    )
    .with_priority(Priority::High)
    .with_cooldown_secs(60);

    let json = serde_json::to_string(&rule).unwrap();
    let deserialized: Rule = serde_json::from_str(&json).unwrap();
    assert_eq!(rule, deserialized);
}

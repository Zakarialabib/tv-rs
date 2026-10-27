//! Scene representation and registry for grouping automated actions.

use crate::action::Action;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A preset scene containing a group of actions to execute simultaneously.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    /// Unique identifier for the scene (e.g. "entrance_queue_alert").
    pub id: String,
    /// Human-readable label for the scene.
    pub name: String,
    /// Actions performed when scene is activated.
    pub actions: Vec<Action>,
}

impl Scene {
    /// Creates a new scene.
    pub fn new(id: impl Into<String>, name: impl Into<String>, actions: Vec<Action>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            actions,
        }
    }
}

/// Registry storing available scenes.
#[derive(Debug, Default, Clone)]
pub struct SceneRegistry {
    scenes: HashMap<String, Scene>,
}

impl SceneRegistry {
    /// Creates an empty `SceneRegistry`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a new scene.
    pub fn register(&mut self, scene: Scene) {
        self.scenes.insert(scene.id.clone(), scene);
    }

    /// Retrieves a scene by ID.
    pub fn get(&self, id: &str) -> Option<&Scene> {
        self.scenes.get(id)
    }

    /// Expands an action: if it's an `ActivateScene`, returns the list of actions from scene,
    /// otherwise returns the original action.
    pub fn expand_action(&self, action: &Action) -> Vec<Action> {
        if let Action::ActivateScene { scene_id } = action {
            if let Some(scene) = self.get(scene_id) {
                scene.actions.clone()
            } else {
                vec![action.clone()]
            }
        } else {
            vec![action.clone()]
        }
    }
}

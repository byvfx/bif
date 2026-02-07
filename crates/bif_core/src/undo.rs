//! Undo/redo system using the command pattern.
//!
//! Provides an `UndoStack` that stores reversible `UndoCommand` objects.
//! Each command knows how to execute and undo itself against an `EditState`.

use std::collections::HashMap;
use std::fmt;

use crate::scene::{AnimatedTransform, Transform, TransformKeyframe};

/// Mutable edit state that commands operate on.
///
/// Stores transform overrides and keyframe overrides keyed by instance index.
#[derive(Debug, Default)]
pub struct EditState {
    /// Per-instance transform overrides (instance_index -> Transform).
    pub transform_overrides: HashMap<usize, Transform>,
    /// Per-instance keyframe overrides (instance_index -> AnimatedTransform).
    pub keyframe_overrides: HashMap<usize, AnimatedTransform>,
}

/// A reversible editing command.
pub trait UndoCommand: fmt::Debug {
    /// Apply this command to the edit state.
    fn execute(&self, state: &mut EditState);
    /// Reverse this command.
    fn undo(&self, state: &mut EditState);
    /// Short description for UI display.
    fn description(&self) -> &str;
}

/// Command that sets a transform override for an instance.
#[derive(Debug)]
pub struct TransformCommand {
    /// Instance index being modified.
    pub instance_index: usize,
    /// Transform before the edit.
    pub old_transform: Transform,
    /// Transform after the edit.
    pub new_transform: Transform,
}

impl UndoCommand for TransformCommand {
    fn execute(&self, state: &mut EditState) {
        state
            .transform_overrides
            .insert(self.instance_index, self.new_transform.clone());
    }

    fn undo(&self, state: &mut EditState) {
        state
            .transform_overrides
            .insert(self.instance_index, self.old_transform.clone());
    }

    fn description(&self) -> &str {
        "Transform"
    }
}

/// Command that modifies keyframes for an instance.
#[derive(Debug)]
pub struct KeyframeCommand {
    /// Instance index being modified.
    pub instance_index: usize,
    /// Keyframes before the edit (None = no animation).
    pub old_keyframes: Option<Vec<TransformKeyframe>>,
    /// Keyframes after the edit.
    pub new_keyframes: Option<Vec<TransformKeyframe>>,
}

impl UndoCommand for KeyframeCommand {
    fn execute(&self, state: &mut EditState) {
        let anim = state
            .keyframe_overrides
            .entry(self.instance_index)
            .or_insert_with(|| AnimatedTransform::static_only(Transform::default()));
        anim.keyframes = self.new_keyframes.clone();
    }

    fn undo(&self, state: &mut EditState) {
        let anim = state
            .keyframe_overrides
            .entry(self.instance_index)
            .or_insert_with(|| AnimatedTransform::static_only(Transform::default()));
        anim.keyframes = self.old_keyframes.clone();
    }

    fn description(&self) -> &str {
        "Set Keyframe"
    }
}

/// Maximum number of undo commands before oldest are dropped.
const MAX_UNDO_COMMANDS: usize = 1000;

/// Stack of undo/redo commands with cursor-based navigation.
#[derive(Debug, Default)]
pub struct UndoStack {
    commands: Vec<Box<dyn UndoCommand>>,
    /// Points to the next command slot (commands[0..cursor] are executed).
    cursor: usize,
}

impl UndoStack {
    /// Create an empty undo stack.
    pub fn new() -> Self {
        Self::default()
    }

    /// Execute a command and push it onto the stack.
    ///
    /// Truncates any redo history beyond the current cursor.
    /// Drops oldest commands when exceeding `MAX_UNDO_COMMANDS`.
    pub fn push(&mut self, cmd: Box<dyn UndoCommand>, state: &mut EditState) {
        // Truncate redo history
        self.commands.truncate(self.cursor);
        cmd.execute(state);
        self.commands.push(cmd);
        self.cursor += 1;

        // Drop oldest commands if over the cap
        if self.commands.len() > MAX_UNDO_COMMANDS {
            let excess = self.commands.len() - MAX_UNDO_COMMANDS;
            self.commands.drain(..excess);
            self.cursor = self.cursor.saturating_sub(excess);
        }
    }

    /// Undo the last command. Returns its description, or None if nothing to undo.
    pub fn undo(&mut self, state: &mut EditState) -> Option<&str> {
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        let cmd = &self.commands[self.cursor];
        cmd.undo(state);
        Some(cmd.description())
    }

    /// Redo the next command. Returns its description, or None if nothing to redo.
    pub fn redo(&mut self, state: &mut EditState) -> Option<&str> {
        if self.cursor >= self.commands.len() {
            return None;
        }
        let cmd = &self.commands[self.cursor];
        cmd.execute(state);
        self.cursor += 1;
        Some(cmd.description())
    }

    /// Whether undo is available.
    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    /// Whether redo is available.
    pub fn can_redo(&self) -> bool {
        self.cursor < self.commands.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bif_math::Vec3;

    #[test]
    fn test_push_and_undo() {
        let mut stack = UndoStack::new();
        let mut state = EditState::default();

        let cmd = TransformCommand {
            instance_index: 0,
            old_transform: Transform::default(),
            new_transform: Transform::from_translation(Vec3::new(5.0, 0.0, 0.0)),
        };

        stack.push(Box::new(cmd), &mut state);
        assert_eq!(state.transform_overrides[&0].translation.x, 5.0);
        assert!(stack.can_undo());
        assert!(!stack.can_redo());

        let desc = stack.undo(&mut state);
        assert_eq!(desc, Some("Transform"));
        assert_eq!(state.transform_overrides[&0].translation.x, 0.0);
        assert!(!stack.can_undo());
        assert!(stack.can_redo());
    }

    #[test]
    fn test_redo() {
        let mut stack = UndoStack::new();
        let mut state = EditState::default();

        let cmd = TransformCommand {
            instance_index: 1,
            old_transform: Transform::default(),
            new_transform: Transform::from_translation(Vec3::new(0.0, 10.0, 0.0)),
        };

        stack.push(Box::new(cmd), &mut state);
        stack.undo(&mut state);
        assert_eq!(state.transform_overrides[&1].translation.y, 0.0);

        let desc = stack.redo(&mut state);
        assert_eq!(desc, Some("Transform"));
        assert_eq!(state.transform_overrides[&1].translation.y, 10.0);
    }

    #[test]
    fn test_push_truncates_redo_history() {
        let mut stack = UndoStack::new();
        let mut state = EditState::default();

        // Push two commands
        stack.push(
            Box::new(TransformCommand {
                instance_index: 0,
                old_transform: Transform::default(),
                new_transform: Transform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
            }),
            &mut state,
        );
        stack.push(
            Box::new(TransformCommand {
                instance_index: 0,
                old_transform: Transform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
                new_transform: Transform::from_translation(Vec3::new(2.0, 0.0, 0.0)),
            }),
            &mut state,
        );

        // Undo once
        stack.undo(&mut state);
        assert_eq!(state.transform_overrides[&0].translation.x, 1.0);

        // Push new command - should truncate redo history
        stack.push(
            Box::new(TransformCommand {
                instance_index: 0,
                old_transform: Transform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
                new_transform: Transform::from_translation(Vec3::new(99.0, 0.0, 0.0)),
            }),
            &mut state,
        );

        assert_eq!(state.transform_overrides[&0].translation.x, 99.0);
        assert!(!stack.can_redo()); // Old redo path gone
    }

    #[test]
    fn test_empty_stack() {
        let mut stack = UndoStack::new();
        let mut state = EditState::default();

        assert!(!stack.can_undo());
        assert!(!stack.can_redo());
        assert!(stack.undo(&mut state).is_none());
        assert!(stack.redo(&mut state).is_none());
    }
}

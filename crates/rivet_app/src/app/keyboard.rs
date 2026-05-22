use eframe::egui::{self, Key};

use crate::types::WorkspaceTab;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutAction {
    OpenNewTask,
    SaveEditor,
    CancelEditor,
    SwitchTab(WorkspaceTab),
    FocusSearch,
    MoveSelection(i32),
    DoneSelected,
    UncompleteSelected,
    DeleteSelected,
    ToggleHelp,
}

pub fn resolve_shortcut(
    ctx: &egui::Context,
    editor_open: bool,
    wants_keyboard_input: bool,
) -> Option<ShortcutAction> {
    for key in [
        Key::F1,
        Key::Slash,
        Key::S,
        Key::Escape,
        Key::N,
        Key::F,
        Key::Num1,
        Key::Num2,
        Key::Num3,
        Key::ArrowDown,
        Key::J,
        Key::ArrowUp,
        Key::K,
        Key::X,
        Key::Backspace,
    ] {
        let pressed = ctx.input(|input| input.key_pressed(key));
        if !pressed {
            continue;
        }
        let command = ctx.input(|input| input.modifiers.command);
        let shift = ctx.input(|input| input.modifiers.shift);
        if let Some(action) = resolve_key_combination(key, command, shift, editor_open, wants_keyboard_input) {
            return Some(action);
        }
    }
    None
}

pub fn resolve_key_combination(
    key: Key,
    command: bool,
    shift: bool,
    editor_open: bool,
    wants_keyboard_input: bool,
) -> Option<ShortcutAction> {
    if matches!(key, Key::F1) || (matches!(key, Key::Slash) && shift) {
        return Some(ShortcutAction::ToggleHelp);
    }
    if editor_open && command && matches!(key, Key::S) {
        return Some(ShortcutAction::SaveEditor);
    }
    if editor_open && matches!(key, Key::Escape) {
        return Some(ShortcutAction::CancelEditor);
    }
    if wants_keyboard_input {
        return None;
    }
    match (key, command, shift) {
        (Key::N, true, _) => Some(ShortcutAction::OpenNewTask),
        (Key::F, true, _) => Some(ShortcutAction::FocusSearch),
        (Key::Num1, true, _) => Some(ShortcutAction::SwitchTab(WorkspaceTab::Tasks)),
        (Key::Num2, true, _) => Some(ShortcutAction::SwitchTab(WorkspaceTab::Kanban)),
        (Key::Num3, true, _) => Some(ShortcutAction::SwitchTab(WorkspaceTab::Calendar)),
        (Key::ArrowDown | Key::J, false, _) => Some(ShortcutAction::MoveSelection(1)),
        (Key::ArrowUp | Key::K, false, _) => Some(ShortcutAction::MoveSelection(-1)),
        (Key::X, false, true) => Some(ShortcutAction::UncompleteSelected),
        (Key::X, false, false) => Some(ShortcutAction::DoneSelected),
        (Key::Backspace, false, _) => Some(ShortcutAction::DeleteSelected),
        _ => None,
    }
}

pub fn move_index(current: Option<usize>, len: usize, delta: i32) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let current = current.unwrap_or(0);
    let next = (current as i32 + delta).clamp(0, len.saturating_sub(1) as i32);
    Some(next as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_index_clamps_bounds() {
        assert_eq!(move_index(None, 0, 1), None);
        assert_eq!(move_index(None, 3, 1), Some(1));
        assert_eq!(move_index(Some(0), 3, -1), Some(0));
        assert_eq!(move_index(Some(2), 3, 1), Some(2));
    }

    #[test]
    fn resolve_key_combination_maps_global_shortcuts() {
        assert_eq!(
            resolve_key_combination(Key::N, true, false, false, false),
            Some(ShortcutAction::OpenNewTask)
        );
        assert_eq!(
            resolve_key_combination(Key::Num2, true, false, false, false),
            Some(ShortcutAction::SwitchTab(WorkspaceTab::Kanban))
        );
        assert_eq!(
            resolve_key_combination(Key::X, false, true, false, false),
            Some(ShortcutAction::UncompleteSelected)
        );
    }

    #[test]
    fn editor_shortcuts_override_text_input_block() {
        assert_eq!(
            resolve_key_combination(Key::S, true, false, true, true),
            Some(ShortcutAction::SaveEditor)
        );
        assert_eq!(
            resolve_key_combination(Key::Escape, false, false, true, true),
            Some(ShortcutAction::CancelEditor)
        );
        assert_eq!(resolve_key_combination(Key::N, true, false, false, true), None);
    }
}

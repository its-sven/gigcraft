use crate::Action;
use serde::{Deserialize, Serialize};

/// How a key was used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Gesture {
    #[default]
    Press,
    DoublePress,
    Hold,
}

/// A key (by name, e.g. "ArrowRight", "PageDown", "Space", "B") with a gesture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub key: String,
    #[serde(default)]
    pub gesture: Gesture,
    pub action: Action,
}

/// Key bindings for perform mode. Stored per device, not in the library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keymap {
    pub bindings: Vec<Binding>,
}

fn b(key: &str, gesture: Gesture, action: Action) -> Binding {
    Binding {
        key: key.to_string(),
        gesture,
        action,
    }
}

impl Default for Keymap {
    /// Works with computer keyboards and the common page turners, which send
    /// arrow keys or Page Up/Down.
    fn default() -> Self {
        use Action::*;
        use Gesture::*;
        Self {
            bindings: vec![
                b("ArrowRight", Press, Next),
                b("Space", Press, Next),
                b("ArrowLeft", Press, Previous),
                b("ArrowRight", Hold, NextSong),
                b("ArrowLeft", Hold, PreviousSong),
                b("PageDown", Press, PageForward),
                b("ArrowDown", Press, PageForward),
                b("PageUp", Press, PageBack),
                b("ArrowUp", Press, PageBack),
                b("Escape", Press, Exit),
                b("P", Press, Playback),
            ],
        }
    }
}

impl Keymap {
    /// The action for `key` and `gesture`. Key names compare case-insensitively.
    #[must_use]
    pub fn action(&self, key: &str, gesture: Gesture) -> Option<Action> {
        self.bindings
            .iter()
            .find(|bd| bd.gesture == gesture && bd.key.eq_ignore_ascii_case(key))
            .map(|bd| bd.action)
    }

    /// Whether `key` has a hold or double-press binding (then a press must wait
    /// to see which gesture it becomes).
    #[must_use]
    pub fn has_extended_gesture(&self, key: &str) -> bool {
        self.bindings
            .iter()
            .any(|bd| bd.gesture != Gesture::Press && bd.key.eq_ignore_ascii_case(key))
    }

    /// Bind `key`+`gesture` to `action`, replacing an existing binding of that key and gesture (MIDI-Learn style).
    pub fn learn(&mut self, key: &str, gesture: Gesture, action: Action) {
        self.bindings
            .retain(|bd| !(bd.gesture == gesture && bd.key.eq_ignore_ascii_case(key)));
        self.bindings.push(Binding {
            key: key.to_string(),
            gesture,
            action,
        });
    }
}

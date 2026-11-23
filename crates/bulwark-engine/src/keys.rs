//! Owned key reservations. Clojure supplies sequences and cancellation strokes.
use crate::{Fault, KeyInput};

pub const MAX_SEQUENCES: usize = 256;
pub const MAX_STROKES: usize = 16;
pub const MAX_STROKE_BYTES: usize = 128;

/// Native interpretation of the Option/Alt modifier, published with reservations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OptionMode {
    #[default]
    PlatformDefault,
    Characters,
    Meta,
}
impl OptionMode {
    pub fn uses_meta(self) -> bool {
        match self {
            Self::PlatformDefault => !cfg!(target_os = "macos"),
            Self::Characters => false,
            Self::Meta => true,
        }
    }
}

#[derive(Debug, Default)]
pub struct KeyRoutes {
    sequences: Vec<Vec<String>>,
    cancel: Vec<String>,
    pub option_mode: OptionMode,
    pub escape_meta: bool,
}
impl KeyRoutes {
    pub fn new(sequences: Vec<Vec<String>>, cancel: Vec<String>) -> Result<Self, Fault> {
        if sequences.len() > MAX_SEQUENCES
            || cancel.len() > MAX_STROKES
            || sequences
                .iter()
                .any(|s| s.is_empty() || s.len() > MAX_STROKES)
            || sequences
                .iter()
                .flatten()
                .chain(cancel.iter())
                .any(|s| s.is_empty() || s.len() > MAX_STROKE_BYTES)
        {
            return Err(Fault::PayloadTooLarge);
        }
        Ok(Self {
            sequences,
            cancel,
            option_mode: OptionMode::default(),
            escape_meta: false,
        })
    }

    /// Advances only on a new non-modifier press. An invalid continuation is
    /// reserved too, so the tail of an application sequence never leaks to a page.
    pub fn capture(&self, pending: &mut Vec<String>, key: &KeyInput) -> bool {
        if !key.pressed
            || matches!(
                key.key.as_str(),
                "Control" | "Alt" | "Shift" | "Meta" | "Super"
            )
        {
            return false;
        }
        let mut stroke = key.stroke();
        if stroke == "Escape" && self.escape_meta {
            if pending.last().is_some_and(|s| s == "Escape") {
                pending.clear();
            } else {
                pending.push(stroke);
            }
            return true;
        }
        if self.cancel.contains(&stroke) {
            pending.clear();
            return true;
        }
        let continuing = !pending.is_empty();
        if pending.last().is_some_and(|s| s == "Escape") {
            pending.pop();
            let mut modified = key.clone();
            modified.alt = true;
            stroke = modified.stroke();
        }
        pending.push(stroke);
        let complete = self.sequences.iter().any(|s| s == pending);
        let prefix = self
            .sequences
            .iter()
            .any(|s| s.len() > pending.len() && s.starts_with(pending));
        if complete || !prefix {
            pending.clear();
        }
        continuing || complete || prefix
    }
}

impl KeyInput {
    /// Same spelling as bulwark.keymap/canonical-stroke. Printable punctuation
    /// already includes Shift; uppercase letters retain an explicit S modifier.
    pub fn stroke(&self) -> String {
        let lower = self.key.to_lowercase();
        let letter = self.key.chars().count() == 1 && lower != self.key.to_uppercase();
        let shift = (letter && self.key != lower)
            || (self.shift && (letter || self.key.chars().count() > 1));
        format!(
            "{}{}{}{}{}",
            if self.control { "C-" } else { "" },
            if self.alt { "M-" } else { "" },
            if shift { "S-" } else { "" },
            if self.meta { "s-" } else { "" },
            if letter { &lower } else { &self.key }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn option_modes_preserve_the_macos_character_default() {
        assert_eq!(
            OptionMode::default().uses_meta(),
            !cfg!(target_os = "macos")
        );
        assert!(!OptionMode::Characters.uses_meta());
        assert!(OptionMode::Meta.uses_meta());
    }
    fn key(text: &str) -> KeyInput {
        KeyInput {
            key: text.into(),
            control: false,
            alt: false,
            shift: false,
            meta: false,
            pressed: true,
        }
    }
    #[test]
    fn sequences_capture_prefixes_invalid_tails_and_cancellation() {
        let routes =
            KeyRoutes::new(vec![vec!["x".into(), "f".into()]], vec!["Escape".into()]).unwrap();
        let mut pending = Vec::new();
        assert!(!routes.capture(&mut pending, &key("a")));
        assert!(routes.capture(&mut pending, &key("x")));
        assert!(!routes.capture(&mut pending, &key("Control")));
        assert!(routes.capture(&mut pending, &key("f")));
        assert!(pending.is_empty());
        assert!(routes.capture(&mut pending, &key("x")));
        assert!(routes.capture(&mut pending, &key("z")));
        assert!(pending.is_empty());
        assert!(!routes.capture(&mut pending, &key("f")));
        assert!(routes.capture(&mut pending, &key("x")));
        assert!(routes.capture(&mut pending, &key("Escape")));
        assert!(pending.is_empty());
    }
    #[test]
    fn escape_meta_reserves_tails_modifiers_cancellation_and_prompt_policy() {
        let mut routes = KeyRoutes::new(
            vec![vec!["M-x".into()], vec!["C-x".into(), "C-M-f".into()]],
            vec!["Escape".into(), "C-g".into()],
        )
        .unwrap();
        routes.escape_meta = true;
        let mut pending = Vec::new();
        assert!(routes.capture(&mut pending, &key("Escape")));
        assert!(!routes.capture(&mut pending, &key("Shift")));
        let mut release = key("Escape");
        release.pressed = false;
        assert!(!routes.capture(&mut pending, &release));
        assert!(routes.capture(&mut pending, &key("x")));
        assert!(pending.is_empty());
        let mut control = key("x");
        control.control = true;
        assert!(routes.capture(&mut pending, &control));
        assert!(routes.capture(&mut pending, &key("Escape")));
        control.key = "f".into();
        assert!(routes.capture(&mut pending, &control));
        assert!(pending.is_empty());
        for tail in [key("Escape"), key("z"), {
            let mut cancel = key("g");
            cancel.control = true;
            cancel
        }] {
            assert!(routes.capture(&mut pending, &key("Escape")));
            assert!(routes.capture(&mut pending, &tail));
            assert!(pending.is_empty());
            assert!(!routes.capture(&mut pending, &key("x")));
        }
        routes.escape_meta = false;
        assert!(routes.capture(&mut pending, &key("Escape")));
        assert!(pending.is_empty());
        assert!(!routes.capture(&mut pending, &key("x")));
    }
    #[test]
    fn spelling_preserves_shifted_letters_punctuation_and_modifier_order() {
        let mut k = key("X");
        k.control = true;
        k.alt = true;
        k.meta = true;
        assert_eq!(k.stroke(), "C-M-S-s-x");
        k = key("<");
        k.alt = true;
        k.shift = true;
        assert_eq!(k.stroke(), "M-<");
        k = key("Enter");
        k.shift = true;
        assert_eq!(k.stroke(), "S-Enter");
        k = key("É");
        assert_eq!(k.stroke(), "S-é");
    }
    #[test]
    fn reservations_have_structural_and_byte_limits() {
        assert!(KeyRoutes::new(vec![vec![]], vec![]).is_err());
        assert!(KeyRoutes::new(vec![vec!["x".into()]; MAX_SEQUENCES + 1], vec![]).is_err());
        assert!(KeyRoutes::new(vec![vec!["x".into(); MAX_STROKES + 1]], vec![]).is_err());
        assert!(KeyRoutes::new(vec![], vec!["é".repeat(MAX_STROKE_BYTES)]).is_err());
    }
}

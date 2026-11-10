//! Owned key reservations. Clojure supplies sequences and cancellation strokes.
use crate::{Fault, KeyInput};

pub const MAX_SEQUENCES: usize = 256;
pub const MAX_STROKES: usize = 16;
pub const MAX_STROKE_BYTES: usize = 128;

#[derive(Debug, Default)]
pub struct KeyRoutes {
    sequences: Vec<Vec<String>>,
    cancel: Vec<String>,
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
        Ok(Self { sequences, cancel })
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
        let stroke = key.stroke();
        if self.cancel.contains(&stroke) {
            pending.clear();
            return true;
        }
        let continuing = !pending.is_empty();
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

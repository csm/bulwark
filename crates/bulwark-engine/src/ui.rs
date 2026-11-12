//! Owned UI descriptions. Clojure supplies rectangles and content; Rust checks geometry,
//! identifiers, and size limits, and renders widgets without commands or bindings.
use crate::{Fault, Handle};

pub const MAX_ELEMENTS: usize = 64;
/// Bounds each coordinate and extent, in logical pixels.
pub const MAX_EXTENT: u32 = 32_768;
pub const MAX_LINE_BYTES: usize = 1024;
pub const MAX_INPUT_BYTES: usize = 4096;
/// Bounds the text of one description, over all elements.
pub const MAX_TEXT_BYTES: usize = 256 * 1024;

/// Logical pixels from the top-left corner of the native window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
impl Rect {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x as f64
            && y >= self.y as f64
            && x < (self.x + self.width) as f64
            && y < (self.y + self.height) as f64
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Widget {
    /// The one visible webview. It is drawn below every other element.
    Web(Handle),
    /// A read-only, scrollable text panel.
    Text(String),
    /// One line of text, such as a modeline.
    Line { text: String, emphasis: bool },
    /// The place of the native echo text. The echo mechanism supplies its content.
    Echo,
    /// A one-line editable control. The text replaces the user's edit only when it
    /// differs from the previous description's text for the same element.
    Input(String),
}
impl Widget {
    fn text_bytes(&self) -> usize {
        match self {
            Self::Text(text) | Self::Line { text, .. } | Self::Input(text) => text.len(),
            Self::Web(_) | Self::Echo => 0,
        }
    }
    /// The parts of a widget that a revision names. Text and emphasis are content.
    fn structure(&self) -> (u8, Option<Handle>) {
        match self {
            Self::Web(view) => (0, Some(*view)),
            Self::Text(_) => (1, None),
            Self::Line { .. } => (2, None),
            Self::Echo => (3, None),
            Self::Input(_) => (4, None),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element {
    /// Interactions name this identifier. Web, text, and input elements require one.
    pub id: Option<u64>,
    pub rect: Rect,
    pub widget: Widget,
}

/// A validated description of the whole native frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ui {
    revision: u64,
    focus: Option<u64>,
    elements: Vec<Element>,
}
impl Ui {
    /// Elements draw in order, so a later element covers an earlier one.
    pub fn new(
        revision: u64,
        focus: Option<u64>,
        elements: Vec<Element>,
    ) -> Result<Self, &'static str> {
        if revision == 0 || revision > i64::MAX as u64 {
            return Err("UI revision must be a positive integer");
        }
        if elements.len() > MAX_ELEMENTS {
            return Err("UI description has more than 64 elements");
        }
        let mut ids = Vec::new();
        let mut web = false;
        let mut bytes = 0usize;
        for element in &elements {
            let Rect {
                x,
                y,
                width,
                height,
            } = element.rect;
            if width == 0 || height == 0 || [x, y, width, height].iter().any(|n| *n > MAX_EXTENT) {
                return Err("UI rectangle needs a positive size and coordinates of at most 32768");
            }
            match (&element.widget, element.id) {
                (_, Some(0)) => return Err("UI element id must be positive"),
                (Widget::Web(_) | Widget::Text(_) | Widget::Input(_), None) => {
                    return Err("Web, text, and input elements need an id");
                }
                _ => {}
            }
            if let Some(id) = element.id {
                if id > i64::MAX as u64 || ids.contains(&id) {
                    return Err("UI element ids must be unique");
                }
                ids.push(id);
            }
            let limit = match &element.widget {
                Widget::Web(_) if std::mem::replace(&mut web, true) => {
                    return Err("UI description has more than one web element");
                }
                Widget::Line { .. } => MAX_LINE_BYTES,
                Widget::Input(_) => MAX_INPUT_BYTES,
                _ => MAX_TEXT_BYTES,
            };
            if element.widget.text_bytes() > limit {
                return Err("UI element text exceeds its limit");
            }
            bytes += element.widget.text_bytes();
        }
        if bytes > MAX_TEXT_BYTES {
            return Err("UI description text exceeds 256 KiB");
        }
        if focus.is_some_and(|focus| !ids.contains(&focus)) {
            return Err("UI focus must name an element");
        }
        Ok(Self {
            revision,
            focus,
            elements,
        })
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn focus(&self) -> Option<u64> {
        self.focus
    }
    pub fn elements(&self) -> &[Element] {
        &self.elements
    }
    pub fn text_bytes(&self) -> usize {
        self.elements.iter().map(|e| e.widget.text_bytes()).sum()
    }
    pub fn element(&self, id: u64) -> Option<&Element> {
        self.elements.iter().find(|e| e.id == Some(id))
    }
    /// The webview that this description shows, and its rectangle.
    pub fn web(&self) -> Option<(Handle, Rect)> {
        self.elements.iter().find_map(|e| match e.widget {
            Widget::Web(view) => Some((view, e.rect)),
            _ => None,
        })
    }
    /// The widget that owns keyboard focus.
    pub fn focused(&self) -> Option<&Widget> {
        self.focus
            .and_then(|id| self.element(id))
            .map(|e| &e.widget)
    }
    /// The element under a logical point: the last one drawn there, with the web element last.
    pub fn hit(&self, x: f64, y: f64) -> Option<&Element> {
        let under = |web: bool| {
            self.elements
                .iter()
                .rev()
                .find(|e| matches!(e.widget, Widget::Web(_)) == web && e.rect.contains(x, y))
        };
        under(false).or_else(|| under(true))
    }
    fn structure(&self) -> impl Iterator<Item = (Option<u64>, Rect, (u8, Option<Handle>))> + '_ {
        self.elements
            .iter()
            .map(|e| (e.id, e.rect, e.widget.structure()))
    }
    /// A revision names one structure: focus, identifiers, rectangles, kinds, and the webview.
    /// A description replaces an older revision, or keeps the revision and changes only content.
    pub fn check_revision(&self, current: Option<&Ui>) -> Result<(), Fault> {
        match current {
            None => Ok(()),
            Some(current) if self.revision > current.revision => Ok(()),
            Some(current)
                if self.revision == current.revision
                    && self.focus == current.focus
                    && self.structure().eq(current.structure()) =>
            {
                Ok(())
            }
            Some(_) => Err(Fault::StaleRevision),
        }
    }
}

/// The native window's drawable area in logical pixels, and its display scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }
    fn element(id: Option<u64>, rect: Rect, widget: Widget) -> Element {
        Element { id, rect, widget }
    }
    fn mixed(revision: u64, text: &str) -> Ui {
        Ui::new(
            revision,
            Some(1),
            vec![
                element(Some(1), rect(0, 0, 500, 710), Widget::Web(Handle(7))),
                element(Some(2), rect(500, 0, 500, 710), Widget::Text(text.into())),
                element(
                    None,
                    rect(0, 710, 1000, 22),
                    Widget::Line {
                        text: text.into(),
                        emphasis: true,
                    },
                ),
                element(None, rect(0, 732, 1000, 28), Widget::Echo),
            ],
        )
        .unwrap()
    }

    #[test]
    fn descriptions_reject_bad_geometry_identifiers_and_sizes() {
        let text = |id, rect| element(id, rect, Widget::Text(String::new()));
        let line = |bytes: usize| Widget::Line {
            text: "x".repeat(bytes),
            emphasis: false,
        };
        let full = rect(0, 0, 10, 10);
        for (revision, focus, elements) in [
            (0, None, vec![]),
            (1, None, vec![text(Some(1), rect(0, 0, 0, 10))]),
            (1, None, vec![text(Some(1), rect(MAX_EXTENT + 1, 0, 1, 1))]),
            (1, None, vec![text(None, full)]),
            (1, None, vec![text(Some(0), full)]),
            (1, None, vec![text(Some(1), full), text(Some(1), full)]),
            (1, Some(2), vec![text(Some(1), full)]),
            (
                1,
                None,
                vec![
                    element(Some(1), full, Widget::Web(Handle(1))),
                    element(Some(2), full, Widget::Web(Handle(2))),
                ],
            ),
            (1, None, vec![element(None, full, line(MAX_LINE_BYTES + 1))]),
            (
                1,
                None,
                vec![element(
                    Some(1),
                    full,
                    Widget::Input("x".repeat(MAX_INPUT_BYTES + 1)),
                )],
            ),
            (
                1,
                None,
                vec![
                    element(Some(1), full, Widget::Text("x".repeat(MAX_TEXT_BYTES))),
                    element(None, full, line(1)),
                ],
            ),
            (
                1,
                None,
                (0..=MAX_ELEMENTS)
                    .map(|_| element(None, full, Widget::Echo))
                    .collect(),
            ),
        ] {
            assert!(
                Ui::new(revision, focus, elements.clone()).is_err(),
                "{elements:?}"
            );
        }
        let exact = Ui::new(
            i64::MAX as u64,
            Some(1),
            vec![
                element(
                    Some(1),
                    rect(MAX_EXTENT, MAX_EXTENT, MAX_EXTENT, MAX_EXTENT),
                    Widget::Text("x".repeat(MAX_TEXT_BYTES - MAX_LINE_BYTES)),
                ),
                element(None, full, line(MAX_LINE_BYTES)),
            ],
        )
        .unwrap();
        assert_eq!(exact.text_bytes(), MAX_TEXT_BYTES);
    }

    #[test]
    fn pointer_hits_follow_draw_order_with_the_web_element_below() {
        let ui = Ui::new(
            1,
            None,
            vec![
                element(Some(2), rect(400, 0, 200, 100), Widget::Text(String::new())),
                element(Some(1), rect(0, 0, 500, 100), Widget::Web(Handle(7))),
                element(None, rect(0, 90, 1000, 10), Widget::Echo),
            ],
        )
        .unwrap();
        let id = |x, y| ui.hit(x, y).map(|e| (e.id, e.widget.structure().0));
        assert_eq!(id(10.0, 10.0), Some((Some(1), 0)));
        assert_eq!(id(450.0, 10.0), Some((Some(2), 1)), "Text covers the page");
        assert_eq!(id(499.9, 89.9), Some((Some(2), 1)));
        assert_eq!(id(10.0, 95.0), Some((None, 3)), "The echo covers the page");
        assert_eq!(id(600.0, 10.0), None, "The far edge is exclusive");
        assert_eq!(id(-1.0, 10.0), None);
        assert_eq!(ui.web(), Some((Handle(7), rect(0, 0, 500, 100))));
    }

    #[test]
    fn a_revision_names_one_structure_and_older_revisions_are_stale() {
        let current = mixed(4, "old");
        assert_eq!(
            mixed(4, "new content").check_revision(Some(&current)),
            Ok(())
        );
        assert_eq!(mixed(5, "old").check_revision(Some(&current)), Ok(()));
        assert_eq!(mixed(1, "old").check_revision(None), Ok(()));
        assert_eq!(
            mixed(3, "old").check_revision(Some(&current)),
            Err(Fault::StaleRevision)
        );
        let moved = |focus, view, width| {
            Ui::new(
                4,
                focus,
                vec![
                    element(Some(1), rect(0, 0, width, 710), Widget::Web(Handle(view))),
                    element(Some(2), rect(500, 0, 500, 710), Widget::Text("old".into())),
                    element(
                        None,
                        rect(0, 710, 1000, 22),
                        Widget::Line {
                            text: "old".into(),
                            emphasis: true,
                        },
                    ),
                    element(None, rect(0, 732, 1000, 28), Widget::Echo),
                ],
            )
            .unwrap()
        };
        assert_eq!(
            moved(Some(1), 7, 500).check_revision(Some(&current)),
            Ok(())
        );
        for changed in [
            moved(Some(2), 7, 500),
            moved(Some(1), 8, 500),
            moved(Some(1), 7, 400),
        ] {
            assert_eq!(
                changed.check_revision(Some(&current)),
                Err(Fault::StaleRevision)
            );
        }
        assert_eq!(current.focused(), Some(&Widget::Web(Handle(7))));
    }
}

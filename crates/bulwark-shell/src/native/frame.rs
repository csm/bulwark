//! Draws UI descriptions with egui and reports interactions as owned data.
//! No command name, binding, or site rule belongs here.
use super::input::PageRect;
use bulwark_engine::{
    EventKind,
    ui::{MAX_INPUT_BYTES, Ui, Widget},
};
use std::collections::HashMap;

pub const ECHO_HEIGHT: f32 = 28.0;
const PANEL: egui::Color32 = egui::Color32::from_rgb(27, 29, 34);
const LINE: egui::Color32 = egui::Color32::from_rgb(43, 46, 54);
const EMPHASIS: egui::Color32 = egui::Color32::from_rgb(58, 78, 112);

struct Edit {
    described: String,
    text: String,
    // New described text moves the cursor to its end at the next draw.
    replaced: bool,
}
/// The user's text for each input element of the presented description.
#[derive(Default)]
pub struct Edits(HashMap<u64, Edit>);
impl Edits {
    /// Keeps an edit while descriptions repeat the element's text. A new element, or new
    /// described text, replaces it. Edits of absent elements are dropped.
    pub fn sync(&mut self, ui: Option<&Ui>) {
        let mut edits = HashMap::new();
        for element in ui.map(Ui::elements).unwrap_or_default() {
            if let (Some(id), Widget::Input { text, .. }) = (element.id, &element.widget) {
                let edit = match self.0.remove(&id) {
                    Some(edit) if edit.described == *text => edit,
                    _ => Edit {
                        described: text.clone(),
                        text: text.clone(),
                        replaced: true,
                    },
                };
                edits.insert(id, edit);
            }
        }
        self.0 = edits;
    }
}

/// The page's physical rectangle. Without a description, the page fills the window above
/// the echo strip. A described page is clipped to the window; an empty result hides it.
pub fn page_rect(
    size: winit::dpi::PhysicalSize<u32>,
    scale: f64,
    ui: Option<&Ui>,
) -> Option<PageRect> {
    let Some(ui) = ui else {
        let echo = (ECHO_HEIGHT as f64 * scale).round() as u32;
        return Some(PageRect {
            x: 0,
            y: 0,
            width: size.width.max(1),
            height: size.height.saturating_sub(echo).max(1),
        });
    };
    let (_, rect) = ui.web()?;
    // Round shared edges the same way, so adjacent elements neither overlap nor leave a gap.
    let edge = |logical: u32, limit: u32| ((logical as f64 * scale).round() as u32).min(limit);
    let (left, right) = (
        edge(rect.x, size.width),
        edge(rect.x + rect.width, size.width),
    );
    let (top, bottom) = (
        edge(rect.y, size.height),
        edge(rect.y + rect.height, size.height),
    );
    (right > left && bottom > top).then_some(PageRect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

/// The window's size in whole logical pixels.
pub fn viewport(size: winit::dpi::PhysicalSize<u32>, scale: f64) -> bulwark_engine::ui::Viewport {
    bulwark_engine::ui::Viewport {
        width: (size.width as f64 / scale).floor() as u32,
        height: (size.height as f64 / scale).floor() as u32,
        scale: scale as f32,
    }
}

/// Draws the Phase 1 frame: the echo strip below a page that fills the window.
pub fn draw_echo(
    context: &egui::Context,
    mut input: egui::RawInput,
    text: &str,
) -> egui::FullOutput {
    // EguiGlow::run gives the root UI the entire window, which makes egui
    // claim page clicks as well as chrome clicks. Bound the root to chrome.
    if let Some(mut rect) = input.screen_rect {
        rect.min.y = (rect.max.y - ECHO_HEIGHT).max(rect.min.y);
        input.screen_rect = Some(rect);
    }
    context.run_ui(input, |ui| {
        egui::Panel::bottom("echo")
            .exact_size(ECHO_HEIGHT)
            .resizable(false)
            .show_inside(ui, |ui| {
                ui.add(egui::Label::new(text).truncate().selectable(false));
            });
    })
}

fn line(pane: &mut egui::Ui, text: impl Into<egui::WidgetText>) {
    pane.add(egui::Label::new(text).truncate().selectable(false));
}

/// Draws every element except the page, which the host composites below this layer.
/// Pointer routing uses the description's own hit test, not egui's.
pub fn draw(
    context: &egui::Context,
    input: egui::RawInput,
    echo: &str,
    ui: &Ui,
    edits: &mut Edits,
    interactions: &mut Vec<EventKind>,
) -> egui::FullOutput {
    context.run_ui(input, |root| {
        for (index, element) in ui.elements().iter().enumerate() {
            let fill = match &element.widget {
                Widget::Web(_) => continue,
                Widget::Line { emphasis: true, .. } => EMPHASIS,
                Widget::Line { .. } => LINE,
                Widget::Text(_) | Widget::Echo | Widget::Input { .. } => PANEL,
            };
            let rect = egui::Rect::from_min_size(
                egui::pos2(element.rect.x as f32, element.rect.y as f32),
                egui::vec2(element.rect.width as f32, element.rect.height as f32),
            );
            let mut pane = root.new_child(
                egui::UiBuilder::new()
                    .id_salt(("bulwark-element", index))
                    .max_rect(rect.shrink2(egui::vec2(6.0, 3.0))),
            );
            pane.set_clip_rect(rect);
            pane.painter().rect_filled(rect, 0.0, fill);
            match (&element.widget, element.id) {
                (Widget::Line { text, emphasis }, _) => {
                    let text = egui::RichText::new(text);
                    line(&mut pane, if *emphasis { text.strong() } else { text });
                }
                (Widget::Echo, _) => line(&mut pane, echo),
                (Widget::Text(text), id) => {
                    egui::ScrollArea::both()
                        .id_salt(("bulwark-text", id))
                        .auto_shrink(false)
                        .show(&mut pane, |pane| {
                            pane.add(
                                egui::Label::new(egui::RichText::new(text).monospace()).extend(),
                            );
                        });
                }
                (Widget::Input { label, .. }, Some(id)) => {
                    let Some(edit) = edits.0.get_mut(&id) else {
                        continue;
                    };
                    let control = egui::Id::new(("bulwark-input", id));
                    if std::mem::take(&mut edit.replaced) {
                        let mut state =
                            egui::TextEdit::load_state(pane.ctx(), control).unwrap_or_default();
                        let end = egui::text::CCursor::new(edit.text.chars().count());
                        state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::one(end)));
                        state.store(pane.ctx(), control);
                    }
                    let response = pane
                        .horizontal(|pane| {
                            if !label.is_empty() {
                                pane.add(egui::Label::new(label).selectable(false));
                            }
                            pane.add(
                                egui::TextEdit::singleline(&mut edit.text)
                                    .id(control)
                                    .desired_width(f32::INFINITY)
                                    .frame(egui::Frame::NONE)
                                    // A character has at most four bytes.
                                    .char_limit(MAX_INPUT_BYTES / 4),
                            )
                        })
                        .inner;
                    let revision = ui.revision();
                    let text = || edit.text.clone();
                    if response.changed() {
                        interactions.push(EventKind::UiInputChanged {
                            element: id,
                            revision,
                            text: text(),
                        });
                    }
                    if response.lost_focus() && pane.input(|i| i.key_pressed(egui::Key::Enter)) {
                        interactions.push(EventKind::UiInputSubmitted {
                            element: id,
                            revision,
                            text: text(),
                        });
                    }
                    // The description owns the focus. A click elsewhere cannot keep it.
                    match (ui.focus() == Some(id), response.has_focus()) {
                        (true, false) => response.request_focus(),
                        (false, true) => response.surrender_focus(),
                        _ => {}
                    }
                }
                (Widget::Web(_), _) | (Widget::Input { .. }, None) => {}
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bulwark_engine::{
        Handle,
        ui::{Element, Rect},
    };

    fn element(id: Option<u64>, rect: [u32; 4], widget: Widget) -> Element {
        Element {
            id,
            rect: Rect {
                x: rect[0],
                y: rect[1],
                width: rect[2],
                height: rect[3],
            },
            widget,
        }
    }
    fn mixed(revision: u64, focus: u64, input: &str) -> Ui {
        Ui::new(
            revision,
            Some(focus),
            vec![
                element(Some(1), [0, 0, 500, 710], Widget::Web(Handle(7))),
                element(
                    Some(2),
                    [500, 0, 500, 710],
                    Widget::Text("notes\nmore".into()),
                ),
                element(
                    None,
                    [0, 710, 1000, 22],
                    Widget::Line {
                        text: "modeline".into(),
                        emphasis: true,
                    },
                ),
                element(
                    Some(3),
                    [0, 732, 1000, 28],
                    Widget::Input {
                        text: input.into(),
                        label: "Find: ".into(),
                    },
                ),
            ],
        )
        .unwrap()
    }
    fn pass(
        context: &egui::Context,
        ui: &Ui,
        edits: &mut Edits,
        events: Vec<egui::Event>,
    ) -> Vec<EventKind> {
        let mut interactions = Vec::new();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                [1000.0, 760.0].into(),
            )),
            focused: true,
            events,
            ..Default::default()
        };
        let _ = draw(context, input, "echo", ui, edits, &mut interactions);
        interactions
    }
    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn egui_consumes_echo_pointer_input_but_leaves_page_input_available_after_resize() {
        let context = egui::Context::default();
        for (size, point, consumed) in [
            ([1000.0, 760.0], [522.0, 138.0], false),
            ([1000.0, 760.0], [10.0, 746.0], true),
            ([650.0, 450.0], [300.0, 132.0], false),
            ([650.0, 450.0], [10.0, 436.0], true),
        ] {
            // Run another pass to include egui's previous-frame hit-test state.
            for _ in 0..2 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size.into())),
                    events: vec![egui::Event::PointerMoved(point.into())],
                    ..Default::default()
                };
                let _ = draw_echo(&context, input, "Fixture title");
            }
            assert_eq!(
                context.egui_wants_pointer_input(),
                consumed,
                "pointer {point:?} in {size:?}"
            );
        }
    }

    #[test]
    fn default_page_reserves_physical_echo_pixels_at_each_scale_and_never_has_zero_size() {
        let page = |size: (u32, u32), scale| {
            let page = page_rect(size.into(), scale, None).unwrap();
            (page.x, page.y, page.width, page.height)
        };
        assert_eq!(page((1000, 760), 1.0), (0, 0, 1000, 732));
        assert_eq!(page((2000, 1520), 2.0), (0, 0, 2000, 1464));
        assert_eq!(page((0, 0), 2.0), (0, 0, 1, 1));
    }

    #[test]
    fn described_page_scales_to_physical_pixels_and_clips_to_the_window() {
        let ui = |rect| {
            Ui::new(
                1,
                None,
                vec![element(Some(1), rect, Widget::Web(Handle(7)))],
            )
            .unwrap()
        };
        let page = |rect, size: (u32, u32), scale| {
            page_rect(size.into(), scale, Some(&ui(rect))).map(|p| (p.x, p.y, p.width, p.height))
        };
        assert_eq!(
            page([500, 0, 500, 710], (2000, 1520), 2.0),
            Some((1000, 0, 1000, 1420))
        );
        // Fractional scales round shared edges once: 333 * 1.5 = 499.5 for both neighbors.
        assert_eq!(
            page([0, 0, 333, 100], (1500, 1140), 1.5),
            Some((0, 0, 500, 150))
        );
        assert_eq!(
            page([333, 0, 667, 100], (1500, 1140), 1.5),
            Some((500, 0, 1000, 150))
        );
        // A description for a larger window is clipped. A page outside the window is hidden.
        assert_eq!(
            page([500, 0, 500, 710], (800, 600), 1.0),
            Some((500, 0, 300, 600))
        );
        assert_eq!(page([900, 0, 100, 710], (800, 600), 1.0), None);
        let text = Ui::new(
            1,
            None,
            vec![element(
                Some(1),
                [0, 0, 10, 10],
                Widget::Text(String::new()),
            )],
        )
        .unwrap();
        assert_eq!(page_rect((800, 600).into(), 1.0, Some(&text)), None);
        let logical = viewport((1501, 1141).into(), 1.5);
        assert_eq!(
            (logical.width, logical.height, logical.scale),
            (1000, 760, 1.5)
        );
    }

    #[test]
    fn input_edits_survive_content_updates_and_report_changes_and_submission() {
        let context = egui::Context::default();
        let mut edits = Edits::default();
        let ui = mixed(4, 3, "");
        edits.sync(Some(&ui));
        // The first pass gives the described focus to the control.
        assert!(pass(&context, &ui, &mut edits, vec![]).is_empty());
        assert_eq!(
            pass(
                &context,
                &ui,
                &mut edits,
                vec![egui::Event::Text("ab".into())]
            ),
            vec![EventKind::UiInputChanged {
                element: 3,
                revision: 4,
                text: "ab".into()
            }]
        );
        // The same revision with other content keeps the edit.
        let same = mixed(4, 3, "");
        edits.sync(Some(&same));
        assert_eq!(
            pass(&context, &same, &mut edits, vec![key(egui::Key::Enter)]),
            vec![EventKind::UiInputSubmitted {
                element: 3,
                revision: 4,
                text: "ab".into()
            }]
        );
        // The control takes the focus back after a submission.
        assert!(pass(&context, &same, &mut edits, vec![]).is_empty());
        assert_eq!(
            pass(
                &context,
                &same,
                &mut edits,
                vec![egui::Event::Text("c".into())]
            ),
            vec![EventKind::UiInputChanged {
                element: 3,
                revision: 4,
                text: "abc".into()
            }]
        );
        // New described text replaces the edit.
        let replaced = mixed(5, 3, "fresh");
        edits.sync(Some(&replaced));
        assert_eq!(edits.0[&3].text, "fresh");
        // The cursor follows the new text, so typing continues at its end.
        assert!(pass(&context, &replaced, &mut edits, vec![]).is_empty());
        assert_eq!(
            pass(
                &context,
                &replaced,
                &mut edits,
                vec![egui::Event::Text("!".into())]
            ),
            vec![EventKind::UiInputChanged {
                element: 3,
                revision: 5,
                text: "fresh!".into()
            }]
        );
        edits.sync(Some(&mixed(5, 3, "fresh")));
        assert_eq!(edits.0[&3].text, "fresh!", "Repeated text kept the edit");
        let replaced = mixed(5, 3, "fresh again");
        edits.sync(Some(&replaced));
        // An input without the described focus receives no text.
        let unfocused = mixed(6, 1, "fresh again");
        edits.sync(Some(&unfocused));
        let _ = pass(&context, &unfocused, &mut edits, vec![]);
        assert!(
            pass(
                &context,
                &unfocused,
                &mut edits,
                vec![egui::Event::Text("x".into())]
            )
            .is_empty()
        );
        assert_eq!(edits.0[&3].text, "fresh again");
        edits.sync(None);
        assert!(edits.0.is_empty());
    }
}

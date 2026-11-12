/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Native input adapted from Servo’s MPL-2.0 winit example; see ../../NOTICE.md.
use bulwark_engine::{Handle, KeyInput, keys::KeyRoutes};
use servo::{DevicePoint, InputEvent, WebView};
use std::cell::Cell;
use std::sync::Arc;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};

/// The page's rectangle inside the window, in physical pixels from the top-left corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// The owned key event for the runtime, and whether the key is free for the focus owner.
/// A reserved command key is never free.
#[derive(Debug, Default)]
pub struct Routed {
    pub key: Option<KeyInput>,
    pub free: bool,
}

pub struct Input {
    // The pointer in window coordinates, and the same point relative to the page.
    position: Option<(f64, f64)>,
    cursor: Cell<DevicePoint>,
    modifiers: Cell<servo::Modifiers>,
    pointer: PointerRoute,
    keys: KeyboardRoute,
}
impl Default for Input {
    fn default() -> Self {
        Self {
            position: None,
            cursor: Cell::new(DevicePoint::zero()),
            modifiers: Cell::new(servo::Modifiers::empty()),
            pointer: PointerRoute::default(),
            keys: KeyboardRoute::default(),
        }
    }
}
impl Input {
    pub fn reset(&mut self) {
        self.modifiers.set(servo::Modifiers::empty());
        self.pointer = PointerRoute::default();
        self.keys.reset();
    }
    /// Clear page ownership without forgetting physically held modifiers or cursor position.
    pub fn change_view(&mut self) {
        self.pointer.buttons.clear();
        self.keys.reset();
    }
    pub fn update_modifiers(&self, keys: winit::keyboard::ModifiersState) {
        let mut flags = servo::Modifiers::empty();
        flags.set(servo::Modifiers::SHIFT, keys.shift_key());
        flags.set(servo::Modifiers::CONTROL, keys.control_key());
        flags.set(servo::Modifiers::ALT, keys.alt_key());
        flags.set(servo::Modifiers::META, keys.super_key());
        self.modifiers.set(flags);
    }
    /// Synchronize route generation and view before processing another key.
    /// The caller emits an ordered reset event when this returns true.
    pub fn sync_routes(&mut self, routes: Arc<KeyRoutes>, view: Handle) -> bool {
        self.keys.sync(routes, view)
    }
    /// The pointer in physical window coordinates, when it is inside the window.
    pub fn position(&self) -> Option<(f64, f64)> {
        self.position
    }
    /// Recomputes the page-relative pointer after the pointer or the page moves.
    /// Returns true when the page takes the movement.
    pub fn place(&mut self, page: Option<PageRect>) -> bool {
        let inside = match (self.position, page) {
            (Some((x, y)), Some(page)) => {
                let (x, y) = (x - page.x as f64, y - page.y as f64);
                self.cursor.set(DevicePoint::new(x as f32, y as f32));
                x >= 0.0 && y >= 0.0 && x < page.width as f64 && y < page.height as f64
            }
            _ => false,
        };
        let forward = self.pointer.inside || inside || !self.pointer.buttons.is_empty();
        self.pointer.inside = inside;
        forward
    }
    /// Routes one native event. The page receives pointer input inside its rectangle and
    /// free keys. A missing view still routes keys, so commands work without a page.
    /// `consumed` means that chrome covers the pointer.
    pub fn forward(
        &mut self,
        view: Option<&WebView>,
        event: WindowEvent,
        page: Option<PageRect>,
        consumed: bool,
    ) -> Routed {
        let notify = |event: InputEvent| {
            if let Some(view) = view {
                view.notify_input_event(event);
            }
        };
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.position = Some((position.x, position.y));
                if self.place(page) {
                    notify(InputEvent::MouseMove(servo::MouseMoveEvent::new(
                        self.cursor.get().into(),
                    )));
                }
            }
            WindowEvent::MouseInput {
                state: button_state,
                button,
                ..
            } => {
                if !self.pointer.button(button, button_state, consumed) {
                    return Routed::default();
                }
                let button = match button {
                    winit::event::MouseButton::Left => servo::MouseButton::Primary,
                    winit::event::MouseButton::Right => servo::MouseButton::Secondary,
                    winit::event::MouseButton::Middle => servo::MouseButton::Auxiliary,
                    winit::event::MouseButton::Back => servo::MouseButton::Back,
                    winit::event::MouseButton::Forward => servo::MouseButton::Forward,
                    winit::event::MouseButton::Other(n) => servo::MouseButton::Other(n),
                };
                let action = if button_state == ElementState::Pressed {
                    servo::MouseButtonAction::Down
                } else {
                    servo::MouseButtonAction::Up
                };
                notify(InputEvent::MouseButton(servo::MouseButtonEvent::new(
                    action,
                    button,
                    self.cursor.get().into(),
                )));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if !self.pointer.inside || consumed {
                    return Routed::default();
                }
                // Keep line deltas in line units. Servo converts them to pixels.
                let (x, y, mode) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        (x as f64, y as f64, servo::WheelMode::DeltaLine)
                    }
                    MouseScrollDelta::PixelDelta(p) => (p.x, p.y, servo::WheelMode::DeltaPixel),
                };
                notify(InputEvent::Wheel(servo::WheelEvent::new(
                    servo::WheelDelta { x, y, z: 0., mode },
                    self.cursor.get().into(),
                )));
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.update_modifiers(modifiers.state());
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let key = match event.logical_key.clone() {
                    winit::keyboard::Key::Character(text) => {
                        servo::Key::Character(text.to_string())
                    }
                    winit::keyboard::Key::Named(winit::keyboard::NamedKey::Space) => {
                        servo::Key::Character(" ".into())
                    }
                    winit::keyboard::Key::Named(named) => format!("{named:?}")
                        .parse()
                        .unwrap_or(servo::Key::Named(servo::NamedKey::Unidentified)),
                    _ => servo::Key::Named(servo::NamedKey::Unidentified),
                };
                let code = match event.physical_key {
                    winit::keyboard::PhysicalKey::Code(code) => format!("{code:?}")
                        .parse()
                        .unwrap_or(servo::Code::Unidentified),
                    _ => servo::Code::Unidentified,
                };
                let key_state = if event.state == ElementState::Pressed {
                    servo::KeyState::Down
                } else {
                    servo::KeyState::Up
                };
                let location = match event.location {
                    winit::keyboard::KeyLocation::Standard => servo::Location::Standard,
                    winit::keyboard::KeyLocation::Left => servo::Location::Left,
                    winit::keyboard::KeyLocation::Right => servo::Location::Right,
                    winit::keyboard::KeyLocation::Numpad => servo::Location::Numpad,
                };
                let flags = self.modifiers.get();
                let owned = KeyInput {
                    key: key.to_string(),
                    control: flags.contains(servo::Modifiers::CONTROL),
                    alt: flags.contains(servo::Modifiers::ALT),
                    shift: flags.contains(servo::Modifiers::SHIFT),
                    meta: flags.contains(servo::Modifiers::META),
                    pressed: event.state == ElementState::Pressed,
                };
                let route = self.keys.route(event.physical_key, &owned, false);
                if route.page {
                    notify(InputEvent::Keyboard(
                        servo::KeyboardEvent::new_without_event(
                            key_state,
                            key,
                            code,
                            location,
                            self.modifiers.get(),
                            event.repeat,
                            false,
                        ),
                    ));
                }
                return Routed {
                    key: route.emit.then_some(owned),
                    free: route.page,
                };
            }
            WindowEvent::CursorLeft { .. } => {
                self.position = None;
                self.pointer.inside = false;
            }
            _ => {}
        }
        Routed::default()
    }
}

#[derive(Default)]
struct KeyboardRoute {
    routes: Arc<KeyRoutes>,
    view: Option<Handle>,
    pending: Vec<String>,
    // Ownership survives keymap updates until release. true means Servo owns it.
    held: Vec<(winit::keyboard::PhysicalKey, bool)>,
}
#[derive(Debug, PartialEq, Eq)]
struct KeyRoute {
    page: bool,
    emit: bool,
}
impl KeyboardRoute {
    fn reset(&mut self) {
        self.pending.clear();
        self.held.clear();
    }
    fn sync(&mut self, routes: Arc<KeyRoutes>, view: Handle) -> bool {
        if self.view == Some(view) && Arc::ptr_eq(&self.routes, &routes) {
            return false;
        }
        if self.view != Some(view) {
            self.held.clear();
        }
        self.routes = routes;
        self.view = Some(view);
        self.pending.clear();
        true
    }
    fn route(
        &mut self,
        physical: winit::keyboard::PhysicalKey,
        key: &KeyInput,
        consumed: bool,
    ) -> KeyRoute {
        if let Some((_, page)) = self.held.iter().find(|(id, _)| *id == physical) {
            let page = *page;
            if !key.pressed {
                self.held.retain(|(id, _)| *id != physical);
            }
            // Repeated presses stay with their owner without advancing the runtime
            // sequence again. Servo still receives ordinary page repeats.
            return KeyRoute {
                page,
                emit: !key.pressed,
            };
        }
        if !key.pressed || consumed {
            return KeyRoute {
                page: false,
                emit: false,
            };
        }
        let page = !self.routes.capture(&mut self.pending, key);
        self.held.push((physical, page));
        KeyRoute { page, emit: true }
    }
}

/// A press belongs to the region in which it starts. Its release follows that
/// owner even when a drag crosses chrome or egui consumes the release.
#[derive(Default)]
struct PointerRoute {
    inside: bool,
    buttons: Vec<winit::event::MouseButton>,
}
impl PointerRoute {
    fn button(
        &mut self,
        button: winit::event::MouseButton,
        state: ElementState,
        consumed: bool,
    ) -> bool {
        match state {
            ElementState::Pressed if self.inside && !consumed => {
                if !self.buttons.contains(&button) {
                    self.buttons.push(button);
                }
                true
            }
            ElementState::Released => {
                let held = self.buttons.contains(&button);
                self.buttons.retain(|held| *held != button);
                held
            }
            _ => false,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use winit::event::MouseButton::Left;
    use winit::keyboard::{KeyCode, PhysicalKey};
    fn key(text: &str, pressed: bool) -> KeyInput {
        KeyInput {
            key: text.into(),
            control: false,
            alt: false,
            shift: false,
            meta: false,
            pressed,
        }
    }
    fn routes() -> Arc<KeyRoutes> {
        Arc::new(KeyRoutes::new(vec![vec!["x".into(), "f".into()]], vec!["Escape".into()]).unwrap())
    }
    #[test]
    fn view_changes_clear_page_ownership_but_preserve_physical_input_state() {
        let mut input = Input::default();
        input.update_modifiers(winit::keyboard::ModifiersState::CONTROL);
        input.pointer.inside = true;
        input.pointer.buttons.push(Left);
        input.cursor.set(DevicePoint::new(25.0, 30.0));
        input.keys.sync(routes(), Handle(1));
        input
            .keys
            .route(PhysicalKey::Code(KeyCode::KeyX), &key("x", true), false);
        input.change_view();
        assert!(input.modifiers.get().contains(servo::Modifiers::CONTROL));
        assert!(input.pointer.inside);
        assert!(input.pointer.buttons.is_empty());
        assert_eq!(input.cursor.get(), DevicePoint::new(25.0, 30.0));
        assert!(input.keys.pending.is_empty());
        assert!(input.keys.held.is_empty());
        input.update_modifiers(winit::keyboard::ModifiersState::empty());
        assert!(input.modifiers.get().is_empty());
    }
    #[test]
    fn pointer_coordinates_are_relative_to_the_page_rectangle() {
        let mut input = Input::default();
        let page = PageRect {
            x: 500,
            y: 20,
            width: 400,
            height: 300,
        };
        assert!(!input.place(Some(page)), "No pointer is inside the window");
        input.position = Some((510.0, 50.0));
        assert!(input.place(Some(page)));
        assert_eq!(input.cursor.get(), DevicePoint::new(10.0, 30.0));
        assert!(input.pointer.inside);
        // The page moved away from a resting pointer. It sees the leave once.
        let moved = PageRect { x: 0, ..page };
        assert!(input.place(Some(moved)));
        assert!(!input.pointer.inside);
        assert!(!input.place(Some(moved)));
        for outside in [(499.0, 50.0), (900.0, 50.0), (510.0, 19.0), (510.0, 320.0)] {
            input.position = Some(outside);
            assert!(!input.place(Some(page)), "{outside:?}");
        }
        input.position = Some((899.0, 319.0));
        assert!(input.place(Some(page)));
        assert!(input.place(None), "A hidden page sees the pointer leave");
        assert!(!input.pointer.inside);
        // A drag that began on the page keeps reaching it outside the rectangle.
        input.pointer.buttons.push(Left);
        input.position = Some((0.0, 0.0));
        assert!(input.place(Some(page)));
    }
    #[test]
    fn command_keys_and_invalid_tails_never_reach_the_page() {
        let mut route = KeyboardRoute::default();
        route.sync(routes(), Handle(1));
        let x = PhysicalKey::Code(KeyCode::KeyX);
        let f = PhysicalKey::Code(KeyCode::KeyF);
        assert_eq!(
            route.route(x, &key("x", true), false),
            KeyRoute {
                page: false,
                emit: true
            }
        );
        assert_eq!(
            route.route(x, &key("x", true), false),
            KeyRoute {
                page: false,
                emit: false
            }
        );
        assert!(!route.route(x, &key("x", false), true).page);
        assert!(!route.route(f, &key("f", true), false).page);
        assert!(route.pending.is_empty());
        assert!(!route.route(f, &key("f", false), false).page);
        route.route(x, &key("x", true), false);
        assert!(!route.route(f, &key("z", true), false).page);
        assert!(route.pending.is_empty());
    }
    #[test]
    fn held_keys_keep_their_owner_across_rebinds_and_chrome_consumption() {
        let mut route = KeyboardRoute::default();
        route.sync(routes(), Handle(1));
        let a = PhysicalKey::Code(KeyCode::KeyA);
        let x = PhysicalKey::Code(KeyCode::KeyX);
        assert!(route.route(a, &key("a", true), false).page);
        assert!(!route.route(x, &key("x", true), false).page);
        route.sync(Arc::default(), Handle(1));
        assert!(!route.route(x, &key("x", false), true).page);
        // A page repeat reaches Servo but cannot corrupt an application prefix.
        assert_eq!(
            route.route(a, &key("a", true), true),
            KeyRoute {
                page: true,
                emit: false
            }
        );
        assert!(route.route(a, &key("a", false), true).page);
        assert!(!route.route(a, &key("a", true), true).page);
        assert!(!route.route(a, &key("a", false), false).page);
    }
    #[test]
    fn focus_view_and_snapshot_changes_cancel_native_prefixes() {
        let mut route = KeyboardRoute::default();
        let table = routes();
        assert!(route.sync(table.clone(), Handle(1)));
        assert!(!route.sync(table.clone(), Handle(1)));
        let x = PhysicalKey::Code(KeyCode::KeyX);
        let f = PhysicalKey::Code(KeyCode::KeyF);
        route.route(x, &key("x", true), false);
        assert!(route.sync(table.clone(), Handle(2)));
        assert!(route.route(f, &key("f", true), false).page);
        route.reset();
        route.route(x, &key("x", true), false);
        route.reset();
        assert!(route.route(f, &key("f", true), false).page);
        route.reset();
        route.route(x, &key("x", true), false);
        assert!(route.sync(routes(), Handle(2)));
        assert!(route.route(f, &key("f", true), false).page);
    }
    #[test]
    fn page_drag_releases_over_chrome_and_chrome_press_never_releases_into_page() {
        let mut route = PointerRoute {
            inside: true,
            ..Default::default()
        };
        assert!(route.button(Left, ElementState::Pressed, false));
        route.inside = false;
        assert!(route.button(Left, ElementState::Released, true));
        assert!(!route.button(Left, ElementState::Pressed, false));
        route.inside = true;
        assert!(!route.button(Left, ElementState::Released, false));
        assert!(!route.button(Left, ElementState::Pressed, true));
        assert!(route.buttons.is_empty());
    }
}

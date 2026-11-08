/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Native input adapted from Servo’s MPL-2.0 winit example; see ../../NOTICE.md.
use bulwark_engine::KeyInput;
use servo::{DevicePoint, InputEvent, WebView};
use std::cell::Cell;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};

pub struct Input {
    cursor: Cell<DevicePoint>,
    modifiers: Cell<servo::Modifiers>,
    pointer: PointerRoute,
    keys: Vec<winit::keyboard::PhysicalKey>,
}
impl Default for Input {
    fn default() -> Self {
        Self {
            cursor: Cell::new(DevicePoint::zero()),
            modifiers: Cell::new(servo::Modifiers::empty()),
            pointer: PointerRoute::default(),
            keys: Vec::new(),
        }
    }
}
impl Input {
    pub fn reset(&mut self) {
        self.modifiers.set(servo::Modifiers::empty());
        self.pointer = PointerRoute::default();
        self.keys.clear();
    }
    pub fn forward(
        &mut self,
        view: &WebView,
        event: WindowEvent,
        size: winit::dpi::PhysicalSize<u32>,
        consumed: bool,
    ) -> Option<KeyInput> {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let point = DevicePoint::new(position.x as f32, position.y as f32);
                self.cursor.set(point);
                let inside = position.x >= 0.0
                    && position.y >= 0.0
                    && position.x < size.width as f64
                    && position.y < size.height as f64;
                let forward = self.pointer.inside || inside || !self.pointer.buttons.is_empty();
                self.pointer.inside = inside;
                if !forward {
                    return None;
                }
                view.notify_input_event(InputEvent::MouseMove(servo::MouseMoveEvent::new(
                    point.into(),
                )));
            }
            WindowEvent::MouseInput {
                state: button_state,
                button,
                ..
            } => {
                if !self.pointer.button(button, button_state, consumed) {
                    return None;
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
                view.notify_input_event(InputEvent::MouseButton(servo::MouseButtonEvent::new(
                    action,
                    button,
                    self.cursor.get().into(),
                )));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if !self.pointer.inside || consumed {
                    return None;
                }
                // Keep line deltas in line units. Servo converts them to pixels.
                let (x, y, mode) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        (x as f64, y as f64, servo::WheelMode::DeltaLine)
                    }
                    MouseScrollDelta::PixelDelta(p) => (p.x, p.y, servo::WheelMode::DeltaPixel),
                };
                view.notify_input_event(InputEvent::Wheel(servo::WheelEvent::new(
                    servo::WheelDelta { x, y, z: 0., mode },
                    self.cursor.get().into(),
                )));
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let keys = modifiers.state();
                let mut flags = servo::Modifiers::empty();
                flags.set(servo::Modifiers::SHIFT, keys.shift_key());
                flags.set(servo::Modifiers::CONTROL, keys.control_key());
                flags.set(servo::Modifiers::ALT, keys.alt_key());
                flags.set(servo::Modifiers::META, keys.super_key());
                self.modifiers.set(flags);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let held = self.keys.contains(&event.physical_key);
                if consumed && !(held && event.state == ElementState::Released) {
                    return None;
                }
                if event.state == ElementState::Pressed {
                    if !held {
                        self.keys.push(event.physical_key);
                    }
                } else {
                    self.keys.retain(|key| *key != event.physical_key);
                }
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
                view.notify_input_event(InputEvent::Keyboard(
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
                return Some(owned);
            }
            WindowEvent::CursorLeft { .. } => self.pointer.inside = false,
            _ => {}
        }
        None
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

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Native input forwarding shared in shape with E3; see its NOTICE.md.
use servo::{DevicePoint, InputEvent, WebView};
use std::cell::Cell;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};

pub struct Input {
    cursor: Cell<DevicePoint>,
    modifiers: Cell<servo::Modifiers>,
}
impl Default for Input {
    fn default() -> Self {
        Self {
            cursor: Cell::new(DevicePoint::zero()),
            modifiers: Cell::new(servo::Modifiers::empty()),
        }
    }
}
impl Input {
    pub fn forward(&self, view: &WebView, event: WindowEvent) {
        match event {
            WindowEvent::Resized(size) => view.resize(size),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                view.set_hidpi_scale_factor(euclid::Scale::new(scale_factor as f32));
            }
            WindowEvent::Focused(focused) => view.set_focused(focused),
            WindowEvent::CursorMoved { position, .. } => {
                let point = DevicePoint::new(position.x as f32, position.y as f32);
                self.cursor.set(point);
                view.notify_input_event(InputEvent::MouseMove(servo::MouseMoveEvent::new(
                    point.into(),
                )));
            }
            WindowEvent::MouseInput {
                state: button_state,
                button,
                ..
            } => {
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
            }
            _ => {}
        }
    }
}

use std::collections::HashSet;

use flecs_ecs::macros::Component;
use glam::DVec2;

use crate::app::window::{Action, Key, Modifiers, MouseButton, WindowEvent};

#[derive(Debug, Component)]
pub struct Keyboard {
    pressed: HashSet<Key>,
    just_pressed: HashSet<Key>,
    just_released: HashSet<Key>,
    just_repeated: HashSet<Key>,
    modifiers: Modifiers,
    text: String,
}

impl Default for Keyboard {
    fn default() -> Self {
        Self {
            pressed: HashSet::new(),
            just_pressed: HashSet::new(),
            just_released: HashSet::new(),
            just_repeated: HashSet::new(),
            modifiers: Modifiers::empty(),
            text: String::new(),
        }
    }
}

impl Keyboard {
    pub fn is_pressed(&self, key: Key) -> bool {
        self.pressed.contains(&key)
    }

    pub fn just_pressed(&self, key: Key) -> bool {
        self.just_pressed.contains(&key)
    }

    pub fn just_released(&self, key: Key) -> bool {
        self.just_released.contains(&key)
    }

    /// Includes OS key repeat.
    pub fn just_pressed_or_repeated(&self, key: Key) -> bool {
        self.just_pressed.contains(&key) || self.just_repeated.contains(&key)
    }

    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn handle_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::Key(key, _, action, mods) => {
                self.modifiers = *mods;
                match action {
                    Action::Press => {
                        self.pressed.insert(*key);
                        self.just_pressed.insert(*key);
                    }
                    Action::Release => {
                        self.pressed.remove(key);
                        self.just_released.insert(*key);
                    }
                    Action::Repeat => {
                        self.just_repeated.insert(*key);
                    }
                }
            }
            WindowEvent::Char(c) => self.text.push(*c),
            _ => {}
        }
    }

    pub(crate) fn clear_frame(&mut self) {
        self.just_pressed.clear();
        self.just_released.clear();
        self.just_repeated.clear();
        self.text.clear();
    }
}

#[derive(Debug, Default, Component)]
pub struct Mouse {
    position: DVec2,
    delta: DVec2,
    scroll_delta: DVec2,
    pressed: HashSet<MouseButton>,
    just_pressed: HashSet<MouseButton>,
    just_released: HashSet<MouseButton>,
}

impl Mouse {
    pub fn position(&self) -> DVec2 {
        self.position
    }

    pub fn delta(&self) -> DVec2 {
        self.delta
    }

    pub fn scroll_delta(&self) -> DVec2 {
        self.scroll_delta
    }

    pub fn is_pressed(&self, button: MouseButton) -> bool {
        self.pressed.contains(&button)
    }

    pub fn just_pressed(&self, button: MouseButton) -> bool {
        self.just_pressed.contains(&button)
    }

    pub fn just_released(&self, button: MouseButton) -> bool {
        self.just_released.contains(&button)
    }

    pub(crate) fn handle_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::CursorPos(x, y) => {
                let new_position = DVec2::new(*x, *y);
                self.delta += new_position - self.position;
                self.position = new_position;
            }
            WindowEvent::MouseButton(button, Action::Press, _) => {
                self.pressed.insert(*button);
                self.just_pressed.insert(*button);
            }
            WindowEvent::MouseButton(button, Action::Release, _) => {
                self.pressed.remove(button);
                self.just_released.insert(*button);
            }
            WindowEvent::Scroll(x, y) => {
                self.scroll_delta += DVec2::new(*x, *y);
            }
            _ => {}
        }
    }

    pub(crate) fn clear_frame(&mut self) {
        self.delta = DVec2::ZERO;
        self.scroll_delta = DVec2::ZERO;
        self.just_pressed.clear();
        self.just_released.clear();
    }
}

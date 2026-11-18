use std::ops::{Deref, DerefMut};

use flecs_ecs::macros::Component;
use glfw::{GlfwReceiver, PWindow};

pub struct WindowDescriptor {
    pub title: &'static str,
    pub width: u32,
    pub height: u32,
    pub mode: WindowMode,
    pub cursor_mode: glfw::CursorMode,
}

impl Default for WindowDescriptor {
    fn default() -> Self {
        Self {
            title: "Blank Game",
            width: 800,
            height: 600,
            mode: WindowMode::Windowed,
            cursor_mode: glfw::CursorMode::Normal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct WindowSize {
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) is_changed: bool,
}

impl WindowSize {
    #[inline]
    pub fn is_changed(&self) -> bool {
        self.is_changed
    }
    
    #[inline]
    pub fn width(&self) -> f32 {
        self.width
    }
    
    #[inline]
    pub fn height(&self) -> f32 {
        self.height
    }
}

/// Singleton giving ECS systems access to the game window.
#[derive(Component, Default)]
pub struct Window(Option<PWindow>, Option<(i32, i32, u32, u32)>);

impl Window {
    pub fn is_fullscreen(&self) -> bool {
        self.with_window_mode(|mode| matches!(mode, glfw::WindowMode::FullScreen(_)))
    }

    pub fn set_fullscreen(&mut self, flag: bool) {
        if flag == self.is_fullscreen() {
            return;
        }

        if flag {
            let (x, y) = self.get_pos();
            let (w, h) = self.get_size();
            self.1 = Some((x, y, w.max(1) as u32, h.max(1) as u32));

            let mut glfw = self.glfw.clone();
            let window = &mut **self;
            glfw.with_primary_monitor(|_, monitor| {
                let Some(monitor) = monitor else {
                    return;
                };
                let Some(mode) = monitor.get_video_mode() else {
                    return;
                };
                window.set_monitor(
                    glfw::WindowMode::FullScreen(monitor),
                    0,
                    0,
                    mode.width,
                    mode.height,
                    Some(mode.refresh_rate),
                );
            });
        } else {
            let (x, y, w, h) = self.1.take().unwrap_or((100, 100, 800, 600));
            self.set_monitor(glfw::WindowMode::Windowed, x, y, w, h, None);
        }
    }

    pub(crate) fn install(&mut self, window: PWindow) {
        self.0 = Some(window);
    }

    pub(crate) fn take(&mut self) -> PWindow {
        self.0.take().expect("Window is only available while the Update pipeline runs")
    }
}

impl Deref for Window {
    type Target = PWindow;

    fn deref(&self) -> &PWindow {
        self.0.as_ref().expect("Window is only available while the Update pipeline runs")
    }
}

impl DerefMut for Window {
    fn deref_mut(&mut self) -> &mut PWindow {
        self.0.as_mut().expect("Window is only available while the Update pipeline runs")
    }
}

pub enum WindowMode {
    Windowed,
    Fullscreen,
}

pub type Events = GlfwReceiver<(f64, WindowEvent)>;

pub use glfw::{Action, CursorMode, Key, Modifiers, MouseButton, Scancode, WindowEvent};
use std::time::{Duration, Instant};

use flecs_ecs::prelude::*;
use glam::Vec2;
use glfw::{Action, Key, Modifiers, MouseButton, WindowEvent};
use image::RgbaImage;
use imgui::{Context, DrawCmd, DrawCmdParams, Key as GuiKey};

pub use imgui as gui;

use crate::{
    Setup,
    app::RenderSlot,
    render::{RenderDevice, registry::RenderRegistry, texture::{TextureDescriptor, TextureHandle}},
    utils::Rect,
};

use super::{
    render::UiOverlay,
    tree::{Batch, Geometry, MODE_TEXTURED, UiQuadVertex},
};

#[derive(Component)]
pub struct ImguiLayer {
    context: Context,
    overlay: UiOverlay,
    font_texture: Option<TextureHandle>,
    last_frame: Instant,
    wants_mouse: bool,
    wants_keyboard: bool,
    version: u64,
}

// SAFETY:
// `Context` is !Send/!Sync only because Dear ImGui keeps global state.
// The layer is created, used and dropped on the thread that runs the game loop.
unsafe impl Send for ImguiLayer {}
unsafe impl Sync for ImguiLayer {}

impl ImguiLayer {
    fn new(overlay: UiOverlay) -> Self {
        let mut context = Context::create();
        context.set_ini_filename(None::<std::path::PathBuf>);

        ImguiLayer {
            context,
            overlay,
            font_texture: None,
            last_frame: Instant::now(),
            wants_mouse: false,
            wants_keyboard: false,
            version: 0,
        }
    }

    pub fn wants_mouse(&self) -> bool {
        self.wants_mouse
    }

    pub fn wants_keyboard(&self) -> bool {
        self.wants_keyboard
    }

    pub fn frame(&mut self, display: Vec2, build: impl FnOnce(&gui::Ui)) {
        let now = Instant::now();
        let delta = now.duration_since(self.last_frame).max(Duration::from_micros(10));
        self.last_frame = now;

        let io = self.context.io_mut();
        io.display_size = [display.x, display.y];
        io.update_delta_time(delta);

        let ui = self.context.new_frame();
        build(ui);

        self.wants_mouse = ui.io().want_capture_mouse;
        self.wants_keyboard = ui.io().want_capture_keyboard;

        let draw_data = self.context.render();
        self.version += 1;

        let mut inner = self.overlay.inner.lock();
        inner.version = self.version;
        fill_geometry(draw_data, self.font_texture, &mut inner.geometry);
    }

    pub fn handle_event(&mut self, event: &WindowEvent) {
        let io = self.context.io_mut();

        match event {
            WindowEvent::CursorPos(x, y) => io.add_mouse_pos_event([*x as f32, *y as f32]),
            WindowEvent::MouseButton(button, action, _) => {
                let button = match button {
                    MouseButton::Button1 => gui::MouseButton::Left,
                    MouseButton::Button2 => gui::MouseButton::Right,
                    MouseButton::Button3 => gui::MouseButton::Middle,
                    _ => return,
                };
                io.add_mouse_button_event(button, *action != Action::Release);
            }
            WindowEvent::Scroll(x, y) => io.add_mouse_wheel_event([*x as f32, *y as f32]),
            WindowEvent::Char(c) => io.add_input_character(*c),
            WindowEvent::Key(key, _, action, mods) => {
                io.add_key_event(GuiKey::ModCtrl, mods.contains(Modifiers::Control));
                io.add_key_event(GuiKey::ModShift, mods.contains(Modifiers::Shift));
                io.add_key_event(GuiKey::ModAlt, mods.contains(Modifiers::Alt));
                io.add_key_event(GuiKey::ModSuper, mods.contains(Modifiers::Super));

                if let Some(key) = map_key(*key) {
                    io.add_key_event(key, *action != Action::Release);
                }
            }
            _ => {}
        }
    }

    fn upload_font(&mut self, registry: &mut RenderRegistry, device: &RenderDevice) {
        let atlas = self.context.fonts().build_rgba32_texture();
        let Some(image) = RgbaImage::from_raw(atlas.width, atlas.height, atlas.data.to_vec()) else {
            log::error!("ImGui font atlas has an unexpected size");
            return;
        };

        let handle = registry.new_texture(device, TextureDescriptor {
            width: atlas.width,
            height: atlas.height,
            label: "ImGui font".to_owned(),
            ..Default::default()
        });

        if let Some(texture) = registry.get_texture(handle) {
            texture.fill(device, &image);
        }

        self.font_texture = Some(handle);
    }
}

fn map_key(key: Key) -> Option<GuiKey> {
    Some(match key {
        Key::Tab => GuiKey::Tab,
        Key::Left => GuiKey::LeftArrow,
        Key::Right => GuiKey::RightArrow,
        Key::Up => GuiKey::UpArrow,
        Key::Down => GuiKey::DownArrow,
        Key::PageUp => GuiKey::PageUp,
        Key::PageDown => GuiKey::PageDown,
        Key::Home => GuiKey::Home,
        Key::End => GuiKey::End,
        Key::Insert => GuiKey::Insert,
        Key::Delete => GuiKey::Delete,
        Key::Backspace => GuiKey::Backspace,
        Key::Space => GuiKey::Space,
        Key::Enter => GuiKey::Enter,
        Key::KpEnter => GuiKey::KeypadEnter,
        Key::Escape => GuiKey::Escape,
        Key::A => GuiKey::A,
        Key::C => GuiKey::C,
        Key::V => GuiKey::V,
        Key::X => GuiKey::X,
        Key::Y => GuiKey::Y,
        Key::Z => GuiKey::Z,
        _ => return None,
    })
}

fn fill_geometry(draw_data: &gui::DrawData, font: Option<TextureHandle>, out: &mut Geometry) {
    out.vertices.clear();
    out.indices.clear();
    out.batches.clear();

    let Some(font) = font else {
        return;
    };

    let origin = Vec2::new(draw_data.display_pos[0], draw_data.display_pos[1]);

    for list in draw_data.draw_lists() {
        let base = out.vertices.len() as u32;
        out.vertices.extend(list.vtx_buffer().iter().map(|v| UiQuadVertex {
            pos: Vec2::new(v.pos[0], v.pos[1]),
            uv: Vec2::new(v.uv[0], v.uv[1]),
            color: [
                v.col[0] as f32 / 255.0,
                v.col[1] as f32 / 255.0,
                v.col[2] as f32 / 255.0,
                v.col[3] as f32 / 255.0,
            ],
            mode: MODE_TEXTURED,
        }));

        let indices = list.idx_buffer();
        for command in list.commands() {
            let DrawCmd::Elements { count, cmd_params } = command else {
                continue;
            };
            let DrawCmdParams { clip_rect, vtx_offset, idx_offset, .. } = cmd_params;

            let start = out.indices.len() as u32;
            out.indices.extend(
                indices[idx_offset..idx_offset + count]
                    .iter()
                    .map(|&i| base + vtx_offset as u32 + i as u32),
            );

            out.batches.push(Batch {
                texture: Some(font),
                scissor: Rect::new(
                    clip_rect[0] - origin.x,
                    clip_rect[1] - origin.y,
                    clip_rect[2] - clip_rect[0],
                    clip_rect[3] - clip_rect[1],
                ),
                start,
                count: count as u32,
            });
        }
    }
}

pub(crate) fn register(world: &World) {
    let overlay = world.get::<&UiOverlay>(|overlay| overlay.clone());

    world.component::<ImguiLayer>().add_trait::<flecs::Singleton>();
    world.set(ImguiLayer::new(overlay));

    world.system::<(&mut ImguiLayer, &mut RenderRegistry, &RenderSlot)>()
        .kind(Setup)
        .each(|(layer, registry, slot)| layer.upload_font(registry, slot.device()));
}

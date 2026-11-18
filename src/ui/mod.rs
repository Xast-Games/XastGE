use ab_glyph::{FontRef, InvalidFont};
use flecs_ecs::prelude::*;

use crate::render::{RenderDevice, registry::{RenderRegistry, RenderRegistryModule}};

pub mod anim;
pub mod app;
pub mod atlas;
pub mod element;
pub mod node;
pub mod render;
pub mod style;
pub mod tree;

pub use app::{Ctx, Inbox, UiApp, UiRoot, UiWorldExt};

use atlas::FontHandle;

pub mod prelude {
    pub use super::{
        Ctx, Inbox, UiApp, UiFonts, UiModule, UiRoot, UiWorldExt,
        element::{Element, PointerEvent, button, col, column, empty, image, row, spacer, text},
        anim::{Ease, Enter},
        style::{Align, Direction, Edges, Justify, Length, Style, VisualPatch, pct, px},
        node::{UiAnim, UiInput, UiLayout, UiLook, UiNode},
        tree::UiTree,
    };
    pub use crate::utils::{Color, Rgba};
}

#[derive(Component, Default)]
pub struct UiFonts {
    pub default: Option<FontHandle>,
}

impl UiFonts {
    /// Builds an atlas per size; first loaded font becomes the default.
    pub fn load(
        &mut self,
        registry: &mut RenderRegistry,
        device: &RenderDevice,
        bytes: &'static [u8],
        sizes: &[u32],
    ) -> Result<FontHandle, InvalidFont> {
        let font = registry.new_font(FontRef::try_from_slice(bytes)?);
        for &size in sizes {
            registry.add_font_atlas(device, font, size);
        }

        self.default.get_or_insert(font);
        Ok(font)
    }
}

#[derive(Component)]
pub struct UiModule;

impl Module for UiModule {
    fn module(world: &World) {
        world.import::<RenderRegistryModule>();

        world.component::<UiFonts>().add_trait::<flecs::Singleton>();
        world.set(UiFonts::default());

        world.component::<app::UiApps>().add_trait::<flecs::Singleton>();
        world.set(app::UiApps::default());

        world.component::<node::UiNode>();
        world.component::<node::UiLook>();
        world.component::<node::UiLayout>();
        world.component::<node::UiInput>();
        world.component::<node::UiAnim>();
        world.component::<tree::UiTree>();

        render::register(world);
    }
}

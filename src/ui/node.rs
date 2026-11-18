use flecs_ecs::prelude::*;
use glam::Vec2;
use taffy::NodeId;

use crate::utils::{Rect, Rgba};

use super::{
    anim::{Ease, Enter, Transition, approach},
    element::ElementKind,
    style::{Style, Visual, VisualPatch},
};

#[derive(Component)]
pub struct UiNode {
    pub(crate) kind: ElementKind,
    pub(crate) key: Option<u64>,
    pub(crate) style: Style,
    pub(crate) layout_id: NodeId,
    pub(crate) children: Vec<Entity>,
    pub(crate) interactive: bool,
}

impl UiNode {
    pub(crate) fn new(kind: ElementKind, layout_id: NodeId) -> Self {
        UiNode {
            kind,
            key: None,
            style: Style::default(),
            layout_id,
            children: Vec::new(),
            interactive: false,
        }
    }

    pub fn kind(&self) -> &ElementKind {
        &self.kind
    }

    pub fn children(&self) -> &[Entity] {
        &self.children
    }
}

#[derive(Component, Default)]
pub struct UiLook {
    pub(crate) visual: Visual,
    pub(crate) hovered_patch: Option<VisualPatch>,
    pub(crate) pressed_patch: Option<VisualPatch>,
}

#[derive(Component, Default)]
pub struct UiLayout {
    pub(crate) intrinsic: Option<Vec2>,
    pub(crate) rect: Rect,
    pub(crate) hit: Option<Rect>,
    pub(crate) clip: Rect,
}

impl UiLayout {
    pub fn rect(&self) -> Rect {
        self.rect
    }
}

#[derive(Component, Default)]
pub struct UiInput {
    pub(crate) hovered: bool,
    pub(crate) pressed: bool,
}

impl UiInput {
    pub fn is_hovered(&self) -> bool {
        self.hovered
    }

    pub fn is_pressed(&self) -> bool {
        self.pressed
    }
}

#[derive(Component)]
pub struct UiAnim {
    pub(crate) transition: Option<Transition>,
    pub(crate) enter: Option<Enter>,
    pub(crate) enter_elapsed: f32,
    pub(crate) hover_t: f32,
    pub(crate) press_t: f32,
    pub(crate) opacity: f32,
    pub(crate) offset: Vec2,
    pub(crate) shift: Vec2,
    pub(crate) slide_from: Vec2,
    pub(crate) slide_to: Vec2,
    pub(crate) slide_t: f32,
    pub(crate) prev_visual: Visual,
    pub(crate) base_t: f32,
}

impl Default for UiAnim {
    fn default() -> Self {
        UiAnim {
            transition: None,
            enter: None,
            enter_elapsed: 0.0,
            hover_t: 0.0,
            press_t: 0.0,
            opacity: 1.0,
            offset: Vec2::ZERO,
            shift: Vec2::ZERO,
            slide_from: Vec2::ZERO,
            slide_to: Vec2::ZERO,
            slide_t: 1.0,
            prev_visual: Visual::default(),
            base_t: 1.0,
        }
    }
}

#[derive(Default)]
pub(crate) struct Tick {
    pub changed: bool,
    pub moved: bool,
    pub running: bool,
}

fn mix(a: Visual, b: Visual, t: f32, inherited: Rgba) -> Visual {
    Visual {
        background: match (a.background, b.background) {
            (Some(from), Some(to)) => Some(from.lerp(to, t)),
            (None, Some(to)) => Some(to.with_alpha(0.0).lerp(to, t)),
            (from, None) => from,
        },
        color: match (a.color, b.color) {
            (Some(from), Some(to)) => Some(from.lerp(to, t)),
            (None, Some(to)) => Some(inherited.lerp(to, t)),
            (from, None) => from,
        },
        edge: match (a.edge, b.edge) {
            (Some(from), Some(to)) => Some(from.lerp(to, t)),
            (None, Some(to)) => Some(to.with_alpha(0.0).lerp(to, t)),
            (from, None) => from,
        },
        shift_x: match (a.shift_x, b.shift_x) {
            (None, None) => None,
            (from, to) => {
                let from = from.unwrap_or(0.0);
                Some(from + (to.unwrap_or(0.0) - from) * t)
            }
        },
        scale: match (a.scale, b.scale) {
            (None, None) => None,
            (from, to) => {
                let from = from.unwrap_or(1.0);
                Some(from + (to.unwrap_or(1.0) - from) * t)
            }
        },
    }
}

impl UiAnim {
    pub(crate) fn animate(&mut self, dt: f32, hovered: bool, pressed: bool) -> Tick {
        let mut tick = Tick::default();
        let duration = self.transition.map_or(0.0, |t| t.duration);
        let ease = self.transition.map_or(Ease::Linear, |t| t.ease);

        let hover_t = approach(self.hover_t, f32::from(hovered), dt, duration);
        let press_t = approach(self.press_t, f32::from(pressed), dt, duration);
        tick.changed |= hover_t != self.hover_t || press_t != self.press_t;
        tick.running |= hover_t != f32::from(hovered) || press_t != f32::from(pressed);
        self.hover_t = hover_t;
        self.press_t = press_t;

        if self.slide_t < 1.0 {
            self.slide_t = approach(self.slide_t, 1.0, dt, duration);
            let shift = self.slide_from.lerp(self.slide_to, ease.apply(self.slide_t));
            tick.changed = true;
            tick.moved |= shift != self.shift;
            tick.running |= self.slide_t < 1.0;
            self.shift = shift;
        }

        if self.base_t < 1.0 {
            self.base_t = approach(self.base_t, 1.0, dt, duration);
            tick.changed = true;
            tick.running |= self.base_t < 1.0;
        }

        if let Some(enter) = self.enter && self.enter_elapsed < enter.delay + enter.duration {
            self.enter_elapsed += dt;

            let progress = (self.enter_elapsed - enter.delay) / enter.duration.max(1e-4);
            let eased = enter.ease.apply(progress);
            let opacity = (enter.opacity + (1.0 - enter.opacity) * eased).clamp(0.0, 1.0);
            let offset = enter.offset * (1.0 - eased);

            tick.changed |= opacity != self.opacity || offset != self.offset;
            tick.moved |= offset != self.offset;
            tick.running = true;
            self.opacity = opacity;
            self.offset = offset;
        }

        tick
    }

    pub(crate) fn base_visual(&self, target: Visual, inherited: Rgba) -> Visual {
        if self.base_t >= 1.0 {
            return target;
        }

        let ease = self.transition.map_or(Ease::Linear, |t| t.ease);
        mix(self.prev_visual, target, ease.apply(self.base_t), inherited)
    }

    pub(crate) fn resolve(&self, look: &UiLook, inherited: Rgba) -> Visual {
        let ease = self.transition.map_or(Ease::Linear, |t| t.ease);
        let mut visual = self.base_visual(look.visual, inherited);

        for (patch, t) in [(&look.hovered_patch, self.hover_t), (&look.pressed_patch, self.press_t)] {
            if let Some(patch) = patch && t > 0.0 {
                let mut target = visual;
                patch.apply(&mut target);
                visual = mix(visual, target, ease.apply(t), inherited);
            }
        }

        visual
    }
}

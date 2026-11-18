use std::collections::{HashMap, VecDeque};

use bytemuck::{Pod, Zeroable};
use flecs_ecs::prelude::*;
use glam::Vec2;
use taffy::{AvailableSpace, NodeId, Size, TaffyTree};

use crate::{
    render::{registry::RenderRegistry, texture::TextureHandle},
    ui::atlas::{Atlas, FontHandle},
    utils::{Rect, Rgba},
};

use super::{
    element::*,
    node::{UiAnim, UiInput, UiLayout, UiLook, UiNode},
    style::{taffy_conv::to_taffy, *},
};

#[derive(Component)]
pub struct UiHandlers<M: Send + Sync + 'static>(pub(crate) Handlers<M>);

const EDGE_WIDTH: f32 = 3.0;

pub(crate) const MODE_TEXTURED: u32 = 0;
pub(crate) const MODE_GLYPH: u32 = 1;

#[repr(C)]
#[derive(Pod, Zeroable, Clone, Copy, Debug)]
pub struct UiQuadVertex {
    pub pos: Vec2,
    pub uv: Vec2,
    pub color: [f32; 4],
    pub mode: u32,
}

#[derive(Clone, Copy)]
struct Xf {
    scale: f32,
    shift: Vec2,
}

impl Xf {
    const IDENTITY: Xf = Xf { scale: 1.0, shift: Vec2::ZERO };

    fn scaled(self, s: f32, rect: Rect) -> Xf {
        if (s - 1.0).abs() < 1e-4 {
            return self;
        }

        let center = Vec2::new(rect.x + rect.w * 0.5, rect.y + rect.h * 0.5);
        Xf { scale: self.scale * s, shift: self.shift + center * (self.scale * (1.0 - s)) }
    }

    fn translated(self, dx: f32) -> Xf {
        Xf { scale: self.scale, shift: self.shift + Vec2::new(dx * self.scale, 0.0) }
    }

    fn rect(self, r: Rect) -> Rect {
        Rect::new(
            r.x * self.scale + self.shift.x,
            r.y * self.scale + self.shift.y,
            r.w * self.scale,
            r.h * self.scale,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Batch {
    /// `None` is solid color.
    pub texture: Option<TextureHandle>,
    pub scissor: Rect,
    pub start: u32,
    pub count: u32,
}

#[derive(Debug, Default)]
pub(crate) struct Geometry {
    pub vertices: Vec<UiQuadVertex>,
    pub indices: Vec<u32>,
    pub batches: Vec<Batch>,
}

impl Geometry {
    fn quad(
        &mut self,
        rect: Rect,
        uv: (Vec2, Vec2),
        color: Rgba,
        mode: u32,
        texture: Option<TextureHandle>,
        scissor: Rect,
    ) {
        if scissor.w <= 0.0 || scissor.h <= 0.0 || rect.intersect(&scissor).w <= 0.0 {
            return;
        }

        let (uv0, uv1) = uv;
        let color = color.to_array();
        let base = self.vertices.len() as u32;

        self.vertices.extend_from_slice(&[
            UiQuadVertex { pos: Vec2::new(rect.x, rect.y), uv: uv0, color, mode },
            UiQuadVertex { pos: Vec2::new(rect.x + rect.w, rect.y), uv: Vec2::new(uv1.x, uv0.y), color, mode },
            UiQuadVertex { pos: Vec2::new(rect.x + rect.w, rect.y + rect.h), uv: uv1, color, mode },
            UiQuadVertex { pos: Vec2::new(rect.x, rect.y + rect.h), uv: Vec2::new(uv0.x, uv1.y), color, mode },
        ]);

        let start = self.indices.len() as u32;
        self.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

        match self.batches.last_mut() {
            Some(last) if last.texture == texture && last.scissor == scissor => last.count += 6,
            _ => self.batches.push(Batch { texture, scissor, start, count: 6 }),
        }
    }

    fn text(&mut self, atlas: &Atlas, content: &str, rect: Rect, color: Rgba, scissor: Rect, scale: f32) {
        let top = rect.y + ((rect.h - atlas.line_height() * scale) * 0.5).max(0.0);
        let baseline = (top + atlas.ascent() * scale).round();
        let mut pen = rect.x;

        for c in content.chars() {
            let Some(glyph) = atlas.glyph(c) else {
                continue;
            };

            if glyph.size != Vec2::ZERO {
                let quad = Rect::new(
                    (pen + glyph.bearing.x * scale).round(),
                    (baseline + glyph.bearing.y * scale).round(),
                    glyph.size.x * scale,
                    glyph.size.y * scale,
                );
                self.quad(quad, (glyph.uv_min, glyph.uv_max), color, MODE_GLYPH, Some(atlas.texture()), scissor);
            }

            pen += glyph.advance * scale;
        }
    }
}

pub(crate) struct Resources<'a> {
    pub registry: Option<&'a RenderRegistry>,
    pub font: Option<FontHandle>,
}

impl<'a> Resources<'a> {
    fn atlas(&self, font_size: u32) -> Option<&'a Atlas> {
        self.registry?.get_atlas_nearest(self.font?, font_size)
    }

    fn intrinsic_size(&self, kind: &ElementKind) -> Option<Vec2> {
        match kind {
            ElementKind::Container => None,
            ElementKind::Text { content, font_size } => Some(
                self.atlas(*font_size)
                    .map_or(Vec2::ZERO, |atlas| atlas.measure_text(content, 0.0)),
            ),
            ElementKind::Image(texture) => Some(
                self.registry
                    .and_then(|r| r.get_texture(*texture))
                    .map_or(Vec2::ZERO, |t| {
                        Vec2::new(t.descriptor().width as f32, t.descriptor().height as f32)
                    }),
            ),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum UiEvent {
    Enter,
    Exit,
    Press,
    Release,
    Click,
    Scroll(Vec2),
    PressAt(PointerEvent),
    Drag(PointerEvent),
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Pointer {
    pub position: Option<Vec2>,
    /// Cursor position even outside the window, for drags.
    pub raw: Vec2,
    pub just_pressed: bool,
    pub just_released: bool,
    pub scroll: Vec2,
}

#[derive(Component)]
pub struct UiTree {
    pub(crate) taffy: TaffyTree<()>,
    pub(crate) root: Option<Entity>,
    pub(crate) viewport: Vec2,
    pub(crate) layout_dirty: bool,
    pub(crate) paint_dirty: bool,
    pub(crate) hovered: Option<Entity>,
    pub(crate) pressed: Option<Entity>,
    pub(crate) drag_last: Option<Vec2>,
    pub(crate) geometry: Geometry,
    pub(crate) version: u64,
    pub z_index: i32,
    pub(crate) animating: bool,
    pub(crate) positions_dirty: bool,
    pub(crate) laid_out: bool,
}

// SAFETY:
// Taffy's !Send/!Sync comes from internal tagged pointer optimizations.
// The `calc` feature, which introduces actual thread-unsafety, is disabled
// in this project.
unsafe impl Send for UiTree {}
unsafe impl Sync for UiTree {}

impl Default for UiTree {
    fn default() -> Self {
        UiTree {
            taffy: TaffyTree::new(),
            root: None,
            viewport: Vec2::ZERO,
            layout_dirty: true,
            paint_dirty: true,
            hovered: None,
            pressed: None,
            drag_last: None,
            geometry: Geometry::default(),
            version: 0,
            z_index: 0,
            animating: false,
            positions_dirty: false,
            laid_out: false,
        }
    }
}

impl UiTree {
    pub fn root(&self) -> Option<Entity> {
        self.root
    }

    pub fn hovered(&self) -> Option<Entity> {
        self.hovered
    }

    // Reconciliation

    pub(crate) fn reconcile<M: Send + Sync + 'static>(
        &mut self,
        world: &World,
        owner: Entity,
        mut view: Element<M>,
    ) {
        if view.style.width == Length::Auto {
            view.style.width = pct(100.0);
        }
        if view.style.height == Length::Auto {
            view.style.height = pct(100.0);
        }

        self.animating = true;
        let (root, _) = self.reconcile_node(world, owner, self.root, view);
        self.root = Some(root);
    }

    fn reconcile_node<M: Send + Sync + 'static>(
        &mut self,
        world: &World,
        parent: Entity,
        existing: Option<Entity>,
        element: Element<M>,
    ) -> (Entity, NodeId) {
        let Element { kind, style, visual, hovered, pressed, transition, enter, offset, key, handlers, children } = element;

        let reusable = existing.filter(|e| {
            world.entity_from_id(*e)
                .try_get::<&UiNode>(|n| n.kind.same_variant(&kind) && n.key == key)
                .unwrap_or(false)
        });

        let (entity, fresh) = match reusable {
            Some(e) => (e, false),
            None => {
                if let Some(old) = existing {
                    self.despawn(world, old);
                }

                let layout_id = self.taffy
                    .new_leaf(taffy::Style::default())
                    .expect("Cannot create UI layout node");

                let entity = world.entity()
                    .child_of(parent)
                    .set(UiNode::new(kind.clone(), layout_id))
                    .set(UiLook::default())
                    .set(UiLayout::default())
                    .set(UiInput::default())
                    .set(UiAnim::default())
                    .id();

                (entity, true)
            }
        };

        let old_children = world.entity_from_id(entity).get::<&UiNode>(|n| n.children.clone());
        let mut keyed = HashMap::new();
        let mut unkeyed = VecDeque::new();
        let mut leftover = Vec::new();

        for child in old_children {
            match world.entity_from_id(child).get::<&UiNode>(|n| n.key) {
                Some(k) => {
                    if let Some(dup) = keyed.insert(k, child) {
                        leftover.push(dup);
                    }
                }
                None => unkeyed.push_back(child),
            }
        }

        let mut new_children = Vec::with_capacity(children.len());
        let mut child_ids = Vec::with_capacity(children.len());

        for child in children {
            let previous = match child.key {
                Some(k) => keyed.remove(&k),
                None => unkeyed.pop_front(),
            };
            let (child_entity, child_id) = self.reconcile_node(world, entity, previous, child);
            new_children.push(child_entity);
            child_ids.push(child_id);
        }

        for stale in leftover.into_iter().chain(keyed.into_values()).chain(unkeyed) {
            self.despawn(world, stale);
        }

        let interactive = handlers.is_interactive();

        let layout_id = world.entity_from_id(entity)
            .get::<(&mut UiNode, &mut UiLook, &mut UiLayout, &mut UiAnim)>(|(node, look, layout, anim)| {
                if fresh || node.style != style || node.kind != kind {
                    if node.kind != kind {
                        layout.intrinsic = None;
                    }
                    node.style = style;
                    node.kind = kind;
                    node.key = key;

                    if matches!(node.kind, ElementKind::Container) || layout.intrinsic.is_some() {
                        self.taffy
                            .set_style(node.layout_id, to_taffy(&node.style, layout.intrinsic))
                            .expect("Cannot set UI node style");
                    }

                    self.layout_dirty = true;
                    self.paint_dirty = true;
                }

                if look.visual != visual || look.hovered_patch != hovered || look.pressed_patch != pressed {
                    if look.visual != visual && !fresh && transition.is_some() {
                        anim.prev_visual = anim.base_visual(look.visual, Rgba::WHITE);
                        anim.base_t = 0.0;
                    }
                    look.visual = visual;
                    look.hovered_patch = hovered;
                    look.pressed_patch = pressed;
                    self.paint_dirty = true;
                }

                if node.children != new_children {
                    self.taffy
                        .set_children(node.layout_id, &child_ids)
                        .expect("Cannot set UI node children");
                    node.children = new_children;
                    self.layout_dirty = true;
                    self.paint_dirty = true;
                }

                anim.transition = transition;

                if fresh {
                    anim.shift = offset;
                    anim.slide_from = offset;
                    anim.slide_to = offset;
                    anim.slide_t = 1.0;
                } else if anim.slide_to != offset {
                    anim.slide_from = anim.shift;
                    anim.slide_to = offset;
                    if transition.is_some() {
                        anim.slide_t = 0.0;
                    } else {
                        anim.shift = offset;
                        anim.slide_t = 1.0;
                        self.positions_dirty = true;
                    }
                }

                if fresh && let Some(enter) = enter {
                    anim.opacity = enter.opacity;
                    anim.offset = enter.offset;
                    anim.enter_elapsed = 0.0;
                }
                anim.enter = enter;

                node.interactive = interactive;
                node.layout_id
            });

        world.entity_from_id(entity).set(UiHandlers(handlers));

        (entity, layout_id)
    }

    fn despawn(&mut self, world: &World, entity: Entity) {
        self.forget(world, entity);
        world.entity_from_id(entity).destruct();
        self.layout_dirty = true;
        self.paint_dirty = true;
    }

    fn forget(&mut self, world: &World, entity: Entity) {
        let (layout_id, children) = world.entity_from_id(entity)
            .get::<&UiNode>(|n| (n.layout_id, n.children.clone()));

        for child in children {
            self.forget(world, child);
        }

        let _ = self.taffy.remove(layout_id);

        if self.hovered == Some(entity) {
            self.hovered = None;
        }
        if self.pressed == Some(entity) {
            self.pressed = None;
            self.drag_last = None;
        }
    }

    // Layout

    pub(crate) fn layout(&mut self, world: &World, res: &Resources, viewport: Vec2) {
        if viewport != self.viewport {
            self.viewport = viewport;
            self.layout_dirty = true;
        }

        let Some(root) = self.root else {
            return;
        };

        if !self.layout_dirty {
            if self.positions_dirty && self.laid_out {
                self.apply_layout(world, root, Vec2::ZERO, Rect::new(0.0, 0.0, viewport.x, viewport.y));
                self.positions_dirty = false;
                self.paint_dirty = true;
            }
            return;
        }

        self.prepare_layout(world, res, root);

        let root_id = world.entity_from_id(root).get::<&UiNode>(|n| n.layout_id);
        let available = Size {
            width: AvailableSpace::Definite(viewport.x),
            height: AvailableSpace::Definite(viewport.y),
        };

        if let Err(e) = self.taffy.compute_layout(root_id, available) {
            log::error!("UI layout failed: {e}");
            return;
        }

        self.apply_layout(world, root, Vec2::ZERO, Rect::new(0.0, 0.0, viewport.x, viewport.y));

        self.layout_dirty = false;
        self.positions_dirty = false;
        self.laid_out = true;
        self.paint_dirty = true;
    }

    pub(crate) fn animate(&mut self, world: &World, dt: f32) {
        if !self.animating {
            return;
        }

        let mut running = false;
        if let Some(root) = self.root {
            self.animate_node(world, root, dt, &mut running);
        }
        self.animating = running;
    }

    fn animate_node(&mut self, world: &World, entity: Entity, dt: f32, running: &mut bool) {
        let children = world.entity_from_id(entity)
            .get::<(&UiNode, &UiInput, &mut UiAnim)>(|(node, input, anim)| {
                let tick = anim.animate(dt, input.hovered, input.pressed);
                self.paint_dirty |= tick.changed;
                self.positions_dirty |= tick.moved;
                *running |= tick.running;
                node.children.clone()
            });

        for child in children {
            self.animate_node(world, child, dt, running);
        }
    }

    fn prepare_layout(&mut self, world: &World, res: &Resources, entity: Entity) {
        let children = world.entity_from_id(entity)
            .get::<(&UiNode, &mut UiLayout)>(|(node, layout)| {
                if layout.intrinsic.is_none() && let Some(size) = res.intrinsic_size(&node.kind) {
                    layout.intrinsic = Some(size);
                    self.taffy
                        .set_style(node.layout_id, to_taffy(&node.style, layout.intrinsic))
                        .expect("Cannot set UI node style");
                }
                node.children.clone()
            });

        for child in children {
            self.prepare_layout(world, res, child);
        }
    }

    fn apply_layout(&self, world: &World, entity: Entity, origin: Vec2, clip: Rect) {
        let (rect, child_clip, children) = world.entity_from_id(entity)
            .get::<(&UiNode, &UiAnim, &mut UiLayout)>(|(node, anim, layout)| {
                let computed = self.taffy.layout(node.layout_id).expect("UI node has no layout");
                layout.rect = Rect::new(
                    origin.x + computed.location.x + anim.offset.x + anim.shift.x,
                    origin.y + computed.location.y + anim.offset.y + anim.shift.y,
                    computed.size.width,
                    computed.size.height,
                );
                layout.clip = clip;

                let child_clip = if node.style.clip { clip.intersect(&layout.rect) } else { clip };
                (layout.rect, child_clip, node.children.clone())
            });

        for child in children {
            self.apply_layout(world, child, Vec2::new(rect.x, rect.y), child_clip);
        }
    }

    // Input

    fn hit_test(&self, world: &World, point: Vec2) -> Option<Entity> {
        let mut hit = None;
        if let Some(root) = self.root {
            Self::hit_node(world, root, point, &mut hit);
        }
        hit
    }

    // Pre-order: the last hit is the topmost
    fn hit_node(world: &World, entity: Entity, point: Vec2, hit: &mut Option<Entity>) {
        let (inside, children) = world.entity_from_id(entity).get::<(&UiNode, &UiLayout)>(|(n, l)| {
            (
                n.interactive && l.hit.unwrap_or(l.rect).contains(point) && l.clip.contains(point),
                n.children.clone(),
            )
        });

        if inside {
            *hit = Some(entity);
        }

        for child in children {
            Self::hit_node(world, child, point, hit);
        }
    }

    fn set_flags(world: &World, entity: Entity, f: impl FnOnce(&mut UiInput)) {
        if world.is_alive(entity) {
            world.entity_from_id(entity).try_get::<&mut UiInput>(f);
        }
    }

    fn pointer_event(world: &World, entity: Entity, position: Vec2) -> Option<PointerEvent> {
        if !world.is_alive(entity) {
            return None;
        }

        world.entity_from_id(entity).try_get::<&UiLayout>(|l| PointerEvent {
            local: position - Vec2::new(l.rect.x, l.rect.y),
            size: Vec2::new(l.rect.w, l.rect.h),
        })
    }

    pub(crate) fn process_pointer(&mut self, world: &World, pointer: &Pointer) -> Vec<(Entity, UiEvent)> {
        let mut events = Vec::new();
        let hit = pointer.position.and_then(|p| self.hit_test(world, p));

        if hit != self.hovered {
            if let Some(old) = self.hovered {
                Self::set_flags(world, old, |n| n.hovered = false);
                events.push((old, UiEvent::Exit));
            }
            if let Some(new) = hit {
                Self::set_flags(world, new, |n| n.hovered = true);
                events.push((new, UiEvent::Enter));
            }
            self.hovered = hit;
            self.paint_dirty = true;
            self.animating = true;
        }

        if pointer.just_pressed && let Some(target) = hit {
            Self::set_flags(world, target, |n| n.pressed = true);
            self.pressed = Some(target);
            events.push((target, UiEvent::Press));
            self.drag_last = Some(pointer.raw);
            if let Some(e) = Self::pointer_event(world, target, pointer.raw) {
                events.push((target, UiEvent::PressAt(e)));
            }
            self.paint_dirty = true;
            self.animating = true;
        }

        if let Some(target) = self.pressed && self.drag_last != Some(pointer.raw) {
            self.drag_last = Some(pointer.raw);
            if let Some(e) = Self::pointer_event(world, target, pointer.raw) {
                events.push((target, UiEvent::Drag(e)));
            }
        }

        if pointer.just_released && let Some(target) = self.pressed.take() {
            self.drag_last = None;
            Self::set_flags(world, target, |n| n.pressed = false);
            events.push((target, UiEvent::Release));
            if hit == Some(target) {
                events.push((target, UiEvent::Click));
            }
            self.paint_dirty = true;
            self.animating = true;
        }

        if pointer.scroll != Vec2::ZERO && let Some(target) = hit {
            events.push((target, UiEvent::Scroll(pointer.scroll)));
        }

        events
    }

    // Paint

    pub(crate) fn paint(&mut self, world: &World, res: &Resources) {
        if !self.paint_dirty {
            return;
        }

        let mut geometry = Geometry::default();
        if let Some(root) = self.root {
            Self::paint_node(world, res, root, Rgba::WHITE, 1.0, Xf::IDENTITY, &mut geometry);
        }

        self.geometry = geometry;
        self.version += 1;
        self.paint_dirty = false;
    }

    fn paint_node(
        world: &World,
        res: &Resources,
        entity: Entity,
        inherited: Rgba,
        inherited_opacity: f32,
        xf: Xf,
        out: &mut Geometry,
    ) {
        let (kind, rect, clip, visual, opacity, children) = world.entity_from_id(entity)
            .get::<(&UiNode, &UiLayout, &UiLook, &UiAnim)>(|(n, l, look, anim)| {
                (
                    n.kind.clone(),
                    l.rect,
                    l.clip,
                    anim.resolve(look, inherited),
                    inherited_opacity * anim.opacity,
                    n.children.clone(),
                )
            });

        let color = visual.color.unwrap_or(inherited);
        let xf = xf.scaled(visual.scale.unwrap_or(1.0), rect);
        let rect = xf.rect(rect);
        world.entity_from_id(entity).get::<&mut UiLayout>(|l| l.hit = Some(rect));
        let faded = |c: Rgba| c.with_alpha(c.a * opacity);

        if let Some(background) = visual.background {
            out.quad(rect, (Vec2::ZERO, Vec2::ONE), faded(background), MODE_TEXTURED, None, clip);
        }

        if let Some(edge) = visual.edge {
            let bar = Rect::new(rect.x, rect.y, EDGE_WIDTH * xf.scale, rect.h);
            out.quad(bar, (Vec2::ZERO, Vec2::ONE), faded(edge), MODE_TEXTURED, None, clip);
        }

        match kind {
            ElementKind::Container => {}
            ElementKind::Text { content, font_size } => {
                if let Some(atlas) = res.atlas(font_size) {
                    out.text(atlas, &content, rect, faded(color), clip, xf.scale);
                }
            }
            ElementKind::Image(texture) => {
                out.quad(rect, (Vec2::ZERO, Vec2::ONE), faded(color), MODE_TEXTURED, Some(texture), clip);
            }
        }

        let child_xf = xf.translated(visual.shift_x.unwrap_or(0.0));
        for child in children {
            Self::paint_node(world, res, child, color, opacity, child_xf, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::anim::{Ease, Enter, Transition};

    #[test]
    fn hover_progress_reverses_midway() {
        let mut a = UiAnim::default();
        a.transition = Some(Transition { duration: 0.2, ease: Ease::Linear });

        a.animate(0.1, true, false);
        assert!((a.hover_t - 0.5).abs() < 1e-5);

        let tick = a.animate(0.05, false, false);
        assert!((a.hover_t - 0.25).abs() < 1e-5);
        assert!(tick.running);

        a.animate(0.1, false, false);
        assert_eq!(a.hover_t, 0.0);
    }

    #[test]
    fn offset_slides_to_target() {
        let mut a = UiAnim::default();
        a.transition = Some(Transition { duration: 0.2, ease: Ease::Linear });
        a.slide_to = Vec2::new(100.0, 0.0);
        a.slide_t = 0.0;

        let tick = a.animate(0.1, false, false);
        assert!((a.shift.x - 50.0).abs() < 1e-3);
        assert!(tick.moved && tick.running);

        a.animate(0.2, false, false);
        assert_eq!(a.shift, Vec2::new(100.0, 0.0));
    }

    #[test]
    fn scale_composes_around_the_centre() {
        let r = Rect::new(0.0, 0.0, 100.0, 100.0);
        let xf = Xf::IDENTITY.scaled(2.0, r);
        let out = xf.rect(r);
        assert_eq!((out.x, out.y, out.w, out.h), (-50.0, -50.0, 200.0, 200.0));
    }

    #[test]
    fn no_transition_snaps() {
        let mut a = UiAnim::default();
        a.animate(0.001, true, false);
        assert_eq!(a.hover_t, 1.0);
    }

    #[test]
    fn enter_waits_for_delay_then_completes() {
        let mut a = UiAnim::default();
        a.enter = Some(Enter::fade().slide(Vec2::new(0.0, 10.0)).duration(0.2).delay(0.1).ease(Ease::Linear));
        a.opacity = 0.0;
        a.offset = Vec2::new(0.0, 10.0);

        a.animate(0.05, false, false);
        assert_eq!(a.opacity, 0.0);

        a.animate(0.15, false, false);
        assert!((a.opacity - 0.5).abs() < 1e-5);
        assert!((a.offset.y - 5.0).abs() < 1e-4);

        a.animate(0.2, false, false);
        assert_eq!(a.opacity, 1.0);
        assert_eq!(a.offset, Vec2::ZERO);
        assert!(!a.animate(0.1, false, false).running);
    }
}

#[cfg(test)]
mod pointer_tests {
    use super::*;

    fn pressable_node(world: &World) -> (UiTree, Entity) {
        let id = TaffyTree::<()>::new().new_leaf(taffy::Style::default()).unwrap();
        let mut node = UiNode::new(ElementKind::Container, id);
        node.interactive = true;

        let layout = UiLayout {
            rect: Rect::new(10.0, 10.0, 100.0, 20.0),
            clip: Rect::new(0.0, 0.0, 800.0, 600.0),
            ..Default::default()
        };

        let entity = world.entity().set(node).set(layout).set(UiInput::default()).id();
        let mut tree = UiTree::default();
        tree.root = Some(entity);
        (tree, entity)
    }

    fn at(x: f32, y: f32) -> Pointer {
        Pointer { position: Some(Vec2::new(x, y)), raw: Vec2::new(x, y), ..Default::default() }
    }

    #[test]
    fn press_reports_position_relative_to_the_node() {
        let world = World::new();
        let (mut tree, _) = pressable_node(&world);

        let events = tree.process_pointer(&world, &Pointer { just_pressed: true, ..at(60.0, 20.0) });

        assert!(events.iter().any(|(_, e)| matches!(
            e,
            UiEvent::PressAt(p) if p.local == Vec2::new(50.0, 10.0) && p.size == Vec2::new(100.0, 20.0)
        )));
    }

    #[test]
    fn drag_follows_the_pointer_outside_the_node_and_window() {
        let world = World::new();
        let (mut tree, _) = pressable_node(&world);

        tree.process_pointer(&world, &Pointer { just_pressed: true, ..at(60.0, 20.0) });

        let still = tree.process_pointer(&world, &at(60.0, 20.0));
        assert!(!still.iter().any(|(_, e)| matches!(e, UiEvent::Drag(_))), "no move, no drag");

        let outside = Pointer { position: None, raw: Vec2::new(500.0, 20.0), ..Default::default() };
        let moved = tree.process_pointer(&world, &outside);
        assert!(moved.iter().any(|(_, e)| matches!(e, UiEvent::Drag(p) if p.local.x == 490.0)));

        let released = tree.process_pointer(&world, &Pointer { just_released: true, ..outside });
        assert!(released.iter().any(|(_, e)| matches!(e, UiEvent::Release)));
        assert!(!released.iter().any(|(_, e)| matches!(e, UiEvent::Click)), "released outside: no click");

        let after = tree.process_pointer(&world, &Pointer { raw: Vec2::new(600.0, 20.0), ..Default::default() });
        assert!(!after.iter().any(|(_, e)| matches!(e, UiEvent::Drag(_))), "drag ends on release");
    }
}

use std::{hash::{DefaultHasher, Hash, Hasher}, sync::Arc};

use glam::Vec2;

use crate::{render::texture::TextureHandle, utils::Rgba};

use super::{anim::{Ease, Enter, Transition}, style::*};

pub const DEFAULT_FONT_SIZE: u32 = 24;

#[derive(Debug, Clone, PartialEq)]
pub enum ElementKind {
    Container,
    Text { content: String, font_size: u32 },
    Image(TextureHandle),
}

impl ElementKind {
    pub(crate) fn same_variant(&self, other: &ElementKind) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

pub type ScrollHandler<M> = Arc<dyn Fn(Vec2) -> M + Send + Sync>;

/// Pointer position relative to the node's top-left, and the node's size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerEvent {
    pub local: Vec2,
    pub size: Vec2,
}

pub type PointerHandler<M> = Arc<dyn Fn(PointerEvent) -> M + Send + Sync>;

/// Messages a node sends on interaction.
pub struct Handlers<M> {
    pub on_click: Option<M>,
    pub on_press: Option<M>,
    pub on_release: Option<M>,
    pub on_enter: Option<M>,
    pub on_exit: Option<M>,
    pub on_scroll: Option<ScrollHandler<M>>,
    pub on_press_at: Option<PointerHandler<M>>,
    pub on_drag: Option<PointerHandler<M>>,
}

impl<M> Default for Handlers<M> {
    fn default() -> Self {
        Handlers {
            on_click: None,
            on_press: None,
            on_release: None,
            on_enter: None,
            on_exit: None,
            on_scroll: None,
            on_press_at: None,
            on_drag: None,
        }
    }
}

impl<M: 'static> Handlers<M> {
    pub fn is_interactive(&self) -> bool {
        self.on_click.is_some()
            || self.on_press.is_some()
            || self.on_release.is_some()
            || self.on_enter.is_some()
            || self.on_exit.is_some()
            || self.on_scroll.is_some()
            || self.on_press_at.is_some()
            || self.on_drag.is_some()
    }

    fn map<N: 'static>(self, f: fn(M) -> N) -> Handlers<N> {
        Handlers {
            on_click: self.on_click.map(f),
            on_press: self.on_press.map(f),
            on_release: self.on_release.map(f),
            on_enter: self.on_enter.map(f),
            on_exit: self.on_exit.map(f),
            on_scroll: self.on_scroll.map(|g| -> ScrollHandler<N> {
                Arc::new(move |delta| f(g(delta)))
            }),
            on_press_at: self.on_press_at.map(|g| -> PointerHandler<N> {
                Arc::new(move |e| f(g(e)))
            }),
            on_drag: self.on_drag.map(|g| -> PointerHandler<N> {
                Arc::new(move |e| f(g(e)))
            }),
        }
    }
}

pub struct Element<M> {
    pub(crate) kind: ElementKind,
    pub(crate) style: Style,
    pub(crate) visual: Visual,
    pub(crate) hovered: Option<VisualPatch>,
    pub(crate) pressed: Option<VisualPatch>,
    pub(crate) transition: Option<Transition>,
    pub(crate) enter: Option<Enter>,
    pub(crate) offset: Vec2,
    pub(crate) key: Option<u64>,
    pub(crate) handlers: Handlers<M>,
    pub(crate) children: Vec<Element<M>>,
}

impl<M> Element<M> {
    fn new(kind: ElementKind) -> Self {
        Element {
            kind,
            style: Style::default(),
            visual: Visual::default(),
            hovered: None,
            pressed: None,
            transition: None,
            enter: None,
            offset: Vec2::ZERO,
            key: None,
            handlers: Handlers::default(),
            children: Vec::new(),
        }
    }
}

/// Vertical flex container.
pub fn column<M>() -> Element<M> {
    Element::new(ElementKind::Container)
}

/// Horizontal flex container.
pub fn row<M>() -> Element<M> {
    let mut el = Element::new(ElementKind::Container);
    el.style.direction = Direction::Row;
    el
}

/// Column `span` of 12 wide.
pub fn col<M>(span: u8) -> Element<M> {
    column().width(pct(span.min(12) as f32 / 12.0 * 100.0))
}

/// Single line of text.
pub fn text<M>(content: impl Into<String>) -> Element<M> {
    Element::new(ElementKind::Text { content: content.into(), font_size: DEFAULT_FONT_SIZE })
}

pub fn image<M>(texture: TextureHandle) -> Element<M> {
    Element::new(ElementKind::Image(texture))
}

/// Fills the remaining space.
pub fn spacer<M>() -> Element<M> {
    column().grow(1.0)
}

/// Draws nothing and takes no space.
pub fn empty<M>() -> Element<M> {
    column()
}

pub fn button<M>(content: impl Into<Element<M>>) -> Element<M> {
    row()
        .padding(Edges::xy(16.0, 8.0))
        .align(Align::Center)
        .justify(Justify::Center)
        .background(Rgba::new(0.15, 0.15, 0.18, 0.9))
        .hovered(|v| v.background(Rgba::new(0.25, 0.25, 0.3, 0.95)))
        .pressed(|v| v.background(Rgba::new(0.1, 0.1, 0.12, 1.0)))
        .child(content)
}

impl<M> From<&str> for Element<M> {
    fn from(s: &str) -> Self {
        text(s)
    }
}

impl<M> From<String> for Element<M> {
    fn from(s: String) -> Self {
        text(s)
    }
}

impl<M> Element<M> {
    // Children

    pub fn child(mut self, child: impl Into<Element<M>>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn children<E: Into<Element<M>>>(mut self, children: impl IntoIterator<Item = E>) -> Self {
        self.children.extend(children.into_iter().map(Into::into));
        self
    }

    pub fn child_if<E: Into<Element<M>>>(self, cond: bool, child: impl FnOnce() -> E) -> Self {
        if cond { self.child(child()) } else { self }
    }

    /// Matches this node against the previous view; otherwise matched by position.
    pub fn key(mut self, key: impl Hash) -> Self {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        self.key = Some(hasher.finish());
        self
    }

    // Layout

    pub fn style(mut self, f: impl FnOnce(Style) -> Style) -> Self {
        self.style = f(self.style);
        self
    }

    pub fn direction(mut self, direction: Direction) -> Self {
        self.style.direction = direction;
        self
    }

    pub fn width(mut self, width: Length) -> Self {
        self.style.width = width;
        self
    }

    pub fn height(mut self, height: Length) -> Self {
        self.style.height = height;
        self
    }

    pub fn min_width(mut self, width: Length) -> Self {
        self.style.min_width = width;
        self
    }

    pub fn min_height(mut self, height: Length) -> Self {
        self.style.min_height = height;
        self
    }

    pub fn max_width(mut self, width: Length) -> Self {
        self.style.max_width = width;
        self
    }

    pub fn max_height(mut self, height: Length) -> Self {
        self.style.max_height = height;
        self
    }

    pub fn fill(self) -> Self {
        self.width(pct(100.0)).height(pct(100.0))
    }

    pub fn grow(mut self, grow: f32) -> Self {
        self.style.grow = grow;
        self
    }

    pub fn shrink(mut self, shrink: f32) -> Self {
        self.style.shrink = shrink;
        self
    }

    pub fn padding(mut self, padding: impl Into<Edges>) -> Self {
        self.style.padding = padding.into();
        self
    }

    pub fn margin(mut self, margin: impl Into<Edges>) -> Self {
        self.style.margin = margin.into();
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.style.gap = gap;
        self
    }

    pub fn align(mut self, align: Align) -> Self {
        self.style.align = Some(align);
        self
    }

    pub fn align_self(mut self, align: Align) -> Self {
        self.style.align_self = Some(align);
        self
    }

    pub fn justify(mut self, justify: Justify) -> Self {
        self.style.justify = Some(justify);
        self
    }

    pub fn center(self) -> Self {
        self.align(Align::Center).justify(Justify::Center)
    }

    pub fn clip(mut self) -> Self {
        self.style.clip = true;
        self
    }

    // Text

    /// Uses the closest registered atlas size.
    pub fn font_size(mut self, size: u32) -> Self {
        if let ElementKind::Text { font_size, .. } = &mut self.kind {
            *font_size = size;
        }
        self
    }

    // Visuals

    pub fn background(mut self, color: impl Into<Rgba>) -> Self {
        self.visual.background = Some(color.into());
        self
    }

    /// Text color / image tint, inherited by children.
    pub fn color(mut self, color: impl Into<Rgba>) -> Self {
        self.visual.color = Some(color.into());
        self
    }

    pub fn scale(mut self, scale: f32) -> Self {
        self.visual.scale = Some(scale);
        self
    }

    pub fn edge(mut self, color: impl Into<Rgba>) -> Self {
        self.visual.edge = Some(color.into());
        self
    }

    pub fn shift_x(mut self, shift: f32) -> Self {
        self.visual.shift_x = Some(shift);
        self
    }

    pub fn offset(mut self, offset: Vec2) -> Self {
        self.offset = offset;
        self
    }

    pub fn hovered(mut self, f: impl FnOnce(VisualPatch) -> VisualPatch) -> Self {
        self.hovered = Some(f(self.hovered.unwrap_or_default()));
        self
    }

    pub fn pressed(mut self, f: impl FnOnce(VisualPatch) -> VisualPatch) -> Self {
        self.pressed = Some(f(self.pressed.unwrap_or_default()));
        self
    }

    // Animation

    /// Eases `hovered`/`pressed` changes instead of snapping.
    pub fn transition(mut self, duration: f32, ease: Ease) -> Self {
        self.transition = Some(Transition { duration, ease });
        self
    }

    pub fn enter(mut self, enter: Enter) -> Self {
        self.enter = Some(enter);
        self
    }

    // Events

    pub fn on_click(mut self, msg: M) -> Self {
        self.handlers.on_click = Some(msg);
        self
    }

    pub fn on_press(mut self, msg: M) -> Self {
        self.handlers.on_press = Some(msg);
        self
    }

    pub fn on_release(mut self, msg: M) -> Self {
        self.handlers.on_release = Some(msg);
        self
    }

    pub fn on_enter(mut self, msg: M) -> Self {
        self.handlers.on_enter = Some(msg);
        self
    }

    pub fn on_exit(mut self, msg: M) -> Self {
        self.handlers.on_exit = Some(msg);
        self
    }

    pub fn on_scroll(mut self, f: impl Fn(Vec2) -> M + Send + Sync + 'static) -> Self {
        self.handlers.on_scroll = Some(Arc::new(f));
        self
    }

    pub fn on_press_at(mut self, f: impl Fn(PointerEvent) -> M + Send + Sync + 'static) -> Self {
        self.handlers.on_press_at = Some(Arc::new(f));
        self
    }

    /// While pressed, on every pointer move, even outside the node or the window.
    pub fn on_drag(mut self, f: impl Fn(PointerEvent) -> M + Send + Sync + 'static) -> Self {
        self.handlers.on_drag = Some(Arc::new(f));
        self
    }
}

impl<M: 'static> Element<M> {
    /// E.g. `editor::view(&self.editor).map(Msg::Editor)`.
    pub fn map<N: 'static>(self, f: fn(M) -> N) -> Element<N> {
        Element {
            kind: self.kind,
            style: self.style,
            visual: self.visual,
            hovered: self.hovered,
            pressed: self.pressed,
            transition: self.transition,
            enter: self.enter,
            offset: self.offset,
            key: self.key,
            handlers: self.handlers.map(f),
            children: self.children.into_iter().map(|c| c.map(f)).collect(),
        }
    }
}

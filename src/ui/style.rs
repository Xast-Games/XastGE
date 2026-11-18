use crate::utils::Rgba;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Length {
    #[default]
    Auto,
    Px(f32),
    /// `0.0..=100.0`
    Percent(f32),
}

pub const fn px(value: f32) -> Length {
    Length::Px(value)
}

pub const fn pct(value: f32) -> Length {
    Length::Percent(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Edges {
    pub const ZERO: Edges = Edges::all(0.0);

    pub const fn all(v: f32) -> Edges {
        Edges { top: v, right: v, bottom: v, left: v }
    }

    pub const fn xy(x: f32, y: f32) -> Edges {
        Edges { top: y, right: x, bottom: y, left: x }
    }

    pub const fn top(v: f32) -> Edges {
        Edges { top: v, ..Edges::ZERO }
    }

    pub const fn bottom(v: f32) -> Edges {
        Edges { bottom: v, ..Edges::ZERO }
    }

    pub const fn left(v: f32) -> Edges {
        Edges { left: v, ..Edges::ZERO }
    }

    pub const fn right(v: f32) -> Edges {
        Edges { right: v, ..Edges::ZERO }
    }
}

impl From<f32> for Edges {
    fn from(v: f32) -> Self {
        Edges::all(v)
    }
}

impl From<(f32, f32)> for Edges {
    fn from((x, y): (f32, f32)) -> Self {
        Edges::xy(x, y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    Row,
    #[default]
    Column,
}

/// Cross-axis alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
    Stretch,
}

/// Main-axis distribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Justify {
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    pub direction: Direction,
    pub width: Length,
    pub height: Length,
    pub min_width: Length,
    pub min_height: Length,
    pub max_width: Length,
    pub max_height: Length,
    pub grow: f32,
    pub shrink: f32,
    pub padding: Edges,
    pub margin: Edges,
    pub gap: f32,
    /// Children on the cross axis. `None` is stretch.
    pub align: Option<Align>,
    pub align_self: Option<Align>,
    pub justify: Option<Justify>,
    pub clip: bool,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            direction: Direction::Column,
            width: Length::Auto,
            height: Length::Auto,
            min_width: Length::Auto,
            min_height: Length::Auto,
            max_width: Length::Auto,
            max_height: Length::Auto,
            grow: 0.0,
            shrink: 1.0,
            padding: Edges::ZERO,
            margin: Edges::ZERO,
            gap: 0.0,
            align: None,
            align_self: None,
            justify: None,
            clip: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Visual {
    pub background: Option<Rgba>,
    /// `None` inherits from the parent.
    pub color: Option<Rgba>,
    pub scale: Option<f32>,
    pub edge: Option<Rgba>,
    pub shift_x: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct VisualPatch {
    pub background: Option<Rgba>,
    pub color: Option<Rgba>,
    pub scale: Option<f32>,
    pub edge: Option<Rgba>,
    pub shift_x: Option<f32>,
}

impl VisualPatch {
    pub fn background(mut self, color: impl Into<Rgba>) -> Self {
        self.background = Some(color.into());
        self
    }

    pub fn color(mut self, color: impl Into<Rgba>) -> Self {
        self.color = Some(color.into());
        self
    }

    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = Some(scale);
        self
    }

    pub fn edge(mut self, color: impl Into<Rgba>) -> Self {
        self.edge = Some(color.into());
        self
    }

    pub fn shift_x(mut self, shift: f32) -> Self {
        self.shift_x = Some(shift);
        self
    }

    pub(crate) fn apply(&self, visual: &mut Visual) {
        if let Some(bg) = self.background {
            visual.background = Some(bg);
        }
        if let Some(color) = self.color {
            visual.color = Some(color);
        }
        if let Some(scale) = self.scale {
            visual.scale = Some(scale);
        }
        if let Some(edge) = self.edge {
            visual.edge = Some(edge);
        }
        if let Some(shift) = self.shift_x {
            visual.shift_x = Some(shift);
        }
    }
}

pub(crate) mod taffy_conv {
    use taffy::prelude::*;

    use super::{Align, Direction, Edges, Justify, Length, Style};

    fn dimension(l: Length) -> Dimension {
        match l {
            Length::Auto => Dimension::auto(),
            Length::Px(v) => Dimension::length(v),
            Length::Percent(v) => Dimension::percent(v / 100.0),
        }
    }

    fn rect_lp(e: Edges) -> Rect<LengthPercentage> {
        Rect {
            left: LengthPercentage::length(e.left),
            right: LengthPercentage::length(e.right),
            top: LengthPercentage::length(e.top),
            bottom: LengthPercentage::length(e.bottom),
        }
    }

    fn rect_lpa(e: Edges) -> Rect<LengthPercentageAuto> {
        Rect {
            left: LengthPercentageAuto::length(e.left),
            right: LengthPercentageAuto::length(e.right),
            top: LengthPercentageAuto::length(e.top),
            bottom: LengthPercentageAuto::length(e.bottom),
        }
    }

    fn align(a: Align) -> AlignItems {
        match a {
            Align::Start => AlignItems::FlexStart,
            Align::Center => AlignItems::Center,
            Align::End => AlignItems::FlexEnd,
            Align::Stretch => AlignItems::Stretch,
        }
    }

    fn justify(j: Justify) -> JustifyContent {
        match j {
            Justify::Start => JustifyContent::FlexStart,
            Justify::Center => JustifyContent::Center,
            Justify::End => JustifyContent::FlexEnd,
            Justify::SpaceBetween => JustifyContent::SpaceBetween,
            Justify::SpaceAround => JustifyContent::SpaceAround,
            Justify::SpaceEvenly => JustifyContent::SpaceEvenly,
        }
    }

    /// `intrinsic` replaces `Auto` width/height of leaves.
    pub fn to_taffy(style: &Style, intrinsic: Option<glam::Vec2>) -> taffy::Style {
        let mut width = dimension(style.width);
        let mut height = dimension(style.height);

        if let Some(size) = intrinsic {
            if style.width == Length::Auto {
                width = Dimension::length(size.x);
            }
            if style.height == Length::Auto {
                height = Dimension::length(size.y);
            }
        }

        taffy::Style {
            display: Display::Flex,
            flex_direction: match style.direction {
                Direction::Row => FlexDirection::Row,
                Direction::Column => FlexDirection::Column,
            },
            size: Size { width, height },
            min_size: Size { width: dimension(style.min_width), height: dimension(style.min_height) },
            max_size: Size { width: dimension(style.max_width), height: dimension(style.max_height) },
            flex_grow: style.grow,
            flex_shrink: style.shrink,
            padding: rect_lp(style.padding),
            margin: rect_lpa(style.margin),
            gap: Size {
                width: LengthPercentage::length(style.gap),
                height: LengthPercentage::length(style.gap),
            },
            align_items: style.align.map(align),
            align_self: style.align_self.map(align),
            justify_content: style.justify.map(justify),
            overflow: if style.clip {
                taffy::Point { x: taffy::Overflow::Clip, y: taffy::Overflow::Clip }
            } else {
                taffy::Point { x: taffy::Overflow::Visible, y: taffy::Overflow::Visible }
            },
            ..Default::default()
        }
    }
}

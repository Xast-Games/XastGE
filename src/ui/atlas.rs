use std::collections::HashMap;
use ab_glyph::{Font, FontRef, Glyph, Point, PxScale, ScaleFont, point};
use glam::Vec2;
use image::{GrayImage, Luma};
use slotmap::new_key_type;

use crate::render::texture::TextureHandle;

pub const PADDING: u32 = 2;
pub const CHARSET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz.,;:!?()^АБВГҐДЕЄЖЗИІЇЙКЛМНОПРСТУФХЦЧШЩЬЮЯабвгґдеєжзиіїйклмнопрстуфхцчшщьюя0123456789 ";

new_key_type! {
    pub struct FontHandle;
}

pub struct FontData {
    pub(crate) font: FontRef<'static>,
    pub(crate) atlases: AtlasSet,
}

impl FontData {
    pub fn new(font: FontRef<'static>) -> FontData {
        FontData {
            font,
            atlases: AtlasSet::new(),
        }
    }
}

#[derive(Default)]
pub struct AtlasSet {
    map: HashMap<u32, Atlas>,
}

pub const fn atlas_size(font_size: u32) -> Option<usize> {
    if font_size > 28 { Some(1024) } 
    else if font_size > 0 { Some(512) } 
    else { None }
}

impl AtlasSet {
    pub fn new() -> AtlasSet {
        AtlasSet::default()
    }

    pub fn register_atlas(
        &mut self, 
        font_size: u32, 
        font: &FontRef<'_>,
        texture: TextureHandle,
    ) {
        if self.map.contains_key(&font_size) {
            log::error!("Atlas for font size {} already exists", font_size);
            return;
        }
        let Some(atlas_size) = atlas_size(font_size) else {
            log::error!("Invalid size {} for font atlas", font_size);
            return;
        };

        let scale = PxScale::from(font_size as f32);
        let scaled_font = font.as_scaled(scale);
        
        let mut glyphs = vec![];
        layout(&scaled_font, point(0.0, 0.0), atlas_size as f32, CHARSET, &mut glyphs);

        let mut atlas = Atlas {
            image: GrayImage::new(atlas_size as u32, atlas_size as u32),
            handle: texture,
            font_size,
            ascent: scaled_font.ascent(),
            descent: scaled_font.descent(),
            glyphs: HashMap::new(),
        };

        for (c, g) in glyphs {
            if let Some(og) = scaled_font.outline_glyph(g.clone()) {
                let bounds = og.px_bounds();
                    
                og.draw(|x, y, v| {
                    let x = x as f32 + bounds.min.x;
                    let y = y as f32 + bounds.min.y;
                    if x >= 0.0 && (x as usize) < atlas_size && y >= 0.0 && (y as usize) < atlas_size {
                        atlas.image[(x as u32, y as u32)] = Luma([(v * 255.0) as u8]);
                    }
                });

                let uv_min = Vec2::new(
                    bounds.min.x / atlas_size as f32, 
                    bounds.min.y / atlas_size as f32,
                );

                let uv_max = Vec2::new(
                    (bounds.min.x + bounds.width()) / atlas_size as f32, 
                    (bounds.min.y + bounds.height()) / atlas_size as f32,
                );

                atlas.glyphs.insert(c, GlyphInfo {
                    uv_min,
                    uv_max,
                    size: Vec2::new(bounds.width(), bounds.height()),
                    advance: scaled_font.h_advance(g.id),
                    bearing: Vec2::new(bounds.min.x - g.position.x, bounds.min.y - g.position.y),
                });
            } else {
                if c == ' ' {
                    atlas.glyphs.insert(c, GlyphInfo {
                        uv_min: Vec2::ZERO,
                        uv_max: Vec2::ZERO,
                        size: Vec2::ZERO,
                        advance: scaled_font.h_advance(g.id),
                        bearing: Vec2::ZERO,
                    });
                } else {
                    log::warn!("Failed to outline glyph `{c}`");
                }
            }
        }

        self.map.insert(font_size, atlas);
    }

    pub fn get_atlas(&self, font_size: u32) -> Option<&Atlas> {
        self.map.get(&font_size)
    }

    pub fn nearest_atlas(&self, font_size: u32) -> Option<&Atlas> {
        self.map
            .iter()
            .min_by_key(|(size, _)| (size.abs_diff(font_size), u32::MAX - **size))
            .map(|(_, atlas)| atlas)
    }
}

pub fn layout<F, SF>(
    font: SF,
    position: Point,
    atlas_size: f32,
    text: &str,
    target: &mut Vec<(char, Glyph)>,
) where
    F: Font,
    SF: ScaleFont<F>,
{
    let v_advance = font.height() + font.line_gap();
    let mut caret = position + point(0.0, font.ascent());
    let mut last_glyph: Option<Glyph> = None;
    for c in text.chars() {
        let mut glyph = font.scaled_glyph(c);
        if let Some(previous) = last_glyph.take() {
            caret.x += font.kern(previous.id, glyph.id);
        }
        glyph.position = caret;

        last_glyph = Some(glyph.clone());
        caret.x += font.h_advance(glyph.id) + PADDING as f32;

        if !c.is_whitespace() && caret.x > position.x + atlas_size {
            caret = point(position.x, caret.y + v_advance);
            glyph.position = caret;
            last_glyph = None;
            caret.x += font.h_advance(glyph.id) + PADDING as f32;
        }
        
        target.push((c, glyph));
    }
}

pub struct GlyphInfo {
    pub uv_min: Vec2,
    pub uv_max: Vec2,
    pub size: Vec2,
    pub advance: f32,
    /// Relative to the pen on the baseline, y-down.
    pub bearing: Vec2,
}

pub struct Atlas {
    image: GrayImage,
    handle: TextureHandle,
    font_size: u32,
    ascent: f32,
    descent: f32,
    glyphs: HashMap<char, GlyphInfo>,
}

impl Atlas {
    pub fn glyph(&self, c: char) -> Option<&GlyphInfo> {
        self.glyphs.get(&c)
    }

    pub fn font_size(&self) -> u32 {
        self.font_size
    }

    pub fn ascent(&self) -> f32 {
        self.ascent
    }

    pub fn line_height(&self) -> f32 {
        self.ascent - self.descent
    }

    pub fn texture(&self) -> TextureHandle {
        self.handle
    }

    pub fn image(&self) -> &GrayImage {
        &self.image
    }

    pub fn measure_text(
        &self,
        text: &str,
        horizontal_spacing: f32,
    ) -> Vec2 {
        let mut width = 0.0f32;
        let mut spaced = 0;

        for c in text.chars() {
            let Some(glyph) = self.glyphs.get(&c) else {
                continue;
            };

            width += glyph.advance;
            if glyph.size != Vec2::ZERO {
                width += horizontal_spacing;
                spaced += 1;
            }
        }

        if spaced > 0 {
            width -= horizontal_spacing;
        }

        Vec2::new(width.ceil(), self.line_height().ceil())
    }
}

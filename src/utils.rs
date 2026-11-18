use flecs_ecs::macros::Component;
use glam::{Vec2, Vec3};

#[cfg(feature = "render")]
use anyhow::{Context, Result, bail};
#[cfg(feature = "render")]
use image::GenericImageView;
#[cfg(feature = "render")]
use std::fs;

#[cfg(feature = "render")]
pub fn load_cubemap(path: &str) -> Result<()> {
	let input_path = std::path::Path::new(path);
	if !input_path.exists() {
		bail!("Cubemap source file does not exist: {}", input_path.display());
	}

	let stem = input_path
		.file_stem()
		.and_then(|s| s.to_str())
		.context("Cubemap source path must have a valid UTF-8 file stem")?;

	let output_dir = input_path
		.parent()
		.unwrap_or_else(|| std::path::Path::new("."))
		.join(format!("{stem}_cubemap_faces"));

	fs::create_dir_all(&output_dir)
		.with_context(|| format!("Failed to create output directory `{}`", output_dir.display()))?;

	let image = image::open(path)
		.with_context(|| format!("Failed to open cubemap cross image at `{path}`"))?
		.to_rgba8();

	let width = image.width();
	let height = image.height();

	if width % 4 != 0 || height % 3 != 0 {
		bail!(
			"Cubemap cross image must have a 4:3 grid layout, got {}x{}",
			width,
			height
		);
	}

	let face_size = width / 4;
	if height / 3 != face_size {
		bail!(
			"Cubemap cross image must use square faces, got {}x{} with face size {}x{}",
			width,
			height,
			face_size,
			height / 3
		);
	}

	// Cross layout:
	//       +Y
	// -X +Z +X -Z
	//       -Y
	// Saved names:
	// right(+X), left(-X), top(+Y), bottom(-Y), front(+Z), back(-Z)
	let faces = [
		("right", 2, 1),
		("left", 0, 1),
		("top", 1, 0),
		("bottom", 1, 2),
		("front", 1, 1),
		("back", 3, 1),
	];

	for (name, grid_x, grid_y) in faces {
		let face = image
			.view(grid_x * face_size, grid_y * face_size, face_size, face_size)
			.to_image();

		let out_path = output_dir.join(format!("{name}_{stem}.png"));
		if out_path.exists() {
			bail!(
				"Refusing to overwrite existing cubemap face `{}`",
				out_path.display()
			);
		}

		let tmp_path = output_dir.join(format!("{name}_{stem}.tmp.png"));
		face
			.save(&tmp_path)
			.with_context(|| format!("Failed to save cubemap face temp file `{}`", tmp_path.display()))?;

		fs::rename(&tmp_path, &out_path).with_context(|| {
			format!(
				"Failed to move cubemap face temp file from `{}` to `{}`",
				tmp_path.display(),
				out_path.display()
			)
		})?;
	}

	Ok(())
}

#[derive(Default, Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Rect {
	pub x: f32,
	pub y: f32,
	pub w: f32,
	pub h: f32,
}

impl Rect {
	pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
		Rect { x, y, w, h }
	}

	pub const fn contains(&self, position: Vec2) -> bool {
		position.x >= self.x
			&& position.x <= self.x + self.w
			&& position.y >= self.y
			&& position.y <= self.y + self.h
	}

	pub fn intersect(&self, other: &Rect) -> Rect {
		let x0 = self.x.max(other.x);
		let y0 = self.y.max(other.y);
		let x1 = (self.x + self.w).min(other.x + other.w);
		let y1 = (self.y + self.h).min(other.y + other.h);

		Rect {
			x: x0,
			y: y0,
			w: (x1 - x0).max(0.0),
			h: (y1 - y0).max(0.0),
		}
	}
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const BLACK: Rgba = Rgba::new(0.0, 0.0, 0.0, 1.0);
    pub const WHITE: Rgba = Rgba::new(1.0, 1.0, 1.0, 1.0);
    pub const TRANSPARENT: Rgba = Rgba::new(0.0, 0.0, 0.0, 0.0);

    #[inline]
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Rgba { r, g, b, a }
    }

    #[inline]
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Rgba { r, g, b, a: 1.0 }
    }

    #[inline]
    pub const fn with_alpha(self, a: f32) -> Self {
        Rgba { a, ..self }
    }

    #[inline]
    pub fn lerp(self, other: Rgba, t: f32) -> Rgba {
        Rgba {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }

    #[inline]
    pub const fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

impl From<Color> for Rgba {
    fn from(c: Color) -> Self {
        Rgba::rgb(c.0.x, c.0.y, c.0.z)
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Default, Component)]
pub struct Color(pub Vec3);

impl Color {
    pub const BLACK: Color = Color(Vec3::new(0.0, 0.0, 0.0));
    pub const WHITE: Color = Color(Vec3::new(1.0, 1.0, 1.0));
    pub const GRAY: Color  = Color(Vec3::new(0.5, 0.5, 0.5));

    pub const RED: Color    = Color(Vec3::new(1.0, 0.0, 0.0));
    pub const GREEN: Color  = Color(Vec3::new(0.0, 1.0, 0.0));
    pub const BLUE: Color   = Color(Vec3::new(0.0, 0.0, 1.0));

    pub const YELLOW: Color = Color(Vec3::new(1.0, 1.0, 0.0));
    pub const CYAN: Color   = Color(Vec3::new(0.0, 1.0, 1.0));
    pub const MAGENTA: Color= Color(Vec3::new(1.0, 0.0, 1.0));

    pub const TRANSPARENT: Color = Color(Vec3::new(0.0, 0.0, 0.0));

    #[inline]
    pub fn new(r: f32, g: f32, b: f32) -> Self {
        Color(Vec3::new(r, g, b))
    }

    #[inline]
    pub fn grayscale(v: f32) -> Self {
        Color(Vec3::new(v, v, v))
    }

    #[inline]
    pub fn lerp(self, other: Color, t: f32) -> Color {
        Color(self.0 + (other.0 - self.0) * t)
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Default, Component)]
pub struct Translation(pub Vec3);

impl Translation {
    pub fn lerp(self, other: Translation, t: f32) -> Vec3 {
        self.0 + (other.0 - self.0) * t
    }
}
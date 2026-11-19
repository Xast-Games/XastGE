use glam::{Vec2, Vec3};

use super::{
    source::{HeightSource, LandscapeSettings, LandscapeSource},
    world::{Anchor, Footprint, local_point},
};

#[derive(Debug, Clone, PartialEq)]
pub struct ShapeSettings {
    pub relief_scale: f32,
    pub ambient_detail: f32,
    pub elevation_gain: f32,
    pub blend: f32,
}

impl Default for ShapeSettings {
    fn default() -> Self {
        ShapeSettings {
            relief_scale: 0.35,
            ambient_detail: 0.2,
            elevation_gain: 1.0,
            blend: 80.0,
        }
    }
}

struct Peak {
    center: Vec2,
    height: f32,
    radius: f32,
    sharpness: f32,
}

struct Line {
    points: Vec<Vec3>,
    width: f32,
    sharpness: f32,
}

struct Flat {
    origin: Vec3,
    yaw_degrees: f32,
    footprint: Footprint,
    falloff: f32,
}

struct Spot {
    center: Vec2,
    radius: f32,
    amount: f32,
}

pub struct AuthoredSource {
    baseline: f32,
    max_rise: f32,
    peaks: Vec<Peak>,
    ridges: Vec<Line>,
    valleys: Vec<Line>,
    flats: Vec<Flat>,
    spots: Vec<Spot>,
    relief: LandscapeSource,
    settings: ShapeSettings,
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn profile(distance: f32, radius: f32, sharpness: f32) -> f32 {
    smoothstep(1.0 - distance / radius.max(1e-3)).powf(sharpness.max(0.1))
}

fn smax(a: f32, b: f32, k: f32) -> f32 {
    if k <= 0.0 {
        return a.max(b);
    }
    let h = (0.5 + 0.5 * (a - b) / k).clamp(0.0, 1.0);
    b + (a - b) * h + k * h * (1.0 - h)
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
    -smax(-a, -b, k)
}

fn flat(v: Vec3) -> Vec2 {
    Vec2::new(v.x, v.z)
}

impl Line {
    fn closest(&self, p: Vec2) -> (f32, f32) {
        let mut best = (f32::INFINITY, self.points.first().map_or(0.0, |q| q.y));
        if self.points.len() == 1 {
            return (flat(self.points[0]).distance(p), self.points[0].y);
        }
        for w in self.points.windows(2) {
            let (a, b) = (flat(w[0]), flat(w[1]));
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
            let d = (a + ab * t).distance(p);
            if d < best.0 {
                best = (d, w[0].y + (w[1].y - w[0].y) * t);
            }
        }
        best
    }
}

impl AuthoredSource {
    pub fn new(baseline: f32, anchors: &[Anchor], seed: u64, landscape: &LandscapeSettings, settings: &ShapeSettings) -> Self {
        let mut source = AuthoredSource {
            baseline,
            max_rise: 1.0,
            peaks: Vec::new(),
            ridges: Vec::new(),
            valleys: Vec::new(),
            flats: Vec::new(),
            spots: Vec::new(),
            relief: LandscapeSource::new(seed, landscape),
            settings: settings.clone(),
        };

        for anchor in anchors {
            match anchor {
                Anchor::Peak { position, radius, sharpness, .. } => source.peaks.push(Peak {
                    center: flat(*position),
                    height: position.y,
                    radius: *radius,
                    sharpness: *sharpness,
                }),
                Anchor::Ridge { points, width, sharpness, .. } if !points.is_empty() => {
                    source.ridges.push(Line { points: points.clone(), width: *width, sharpness: *sharpness })
                }
                Anchor::Valley { points, width, .. } if !points.is_empty() => {
                    source.valleys.push(Line { points: points.clone(), width: *width, sharpness: 1.0 })
                }
                Anchor::Plateau { position, yaw_degrees, footprint, falloff, .. } => source.flats.push(Flat {
                    origin: *position,
                    yaw_degrees: *yaw_degrees,
                    footprint: footprint.clone(),
                    falloff: *falloff,
                }),
                Anchor::Detail { position, radius, amount, .. } => {
                    source.spots.push(Spot { center: flat(*position), radius: *radius, amount: *amount })
                }
                _ => {}
            }
        }

        let rises = source.peaks.iter().map(|p| p.height)
            .chain(source.ridges.iter().flat_map(|r| r.points.iter().map(|q| q.y)));
        source.max_rise = rises.map(|h| h - source.baseline).fold(1.0, f32::max);
        source
    }

    pub fn shape(&self, p: Vec2) -> f32 {
        let k = self.settings.blend;
        let mut rise: Option<f32> = None;
        let mut sink: Option<f32> = None;
        let mut add = |c: f32| {
            if c > 0.0 {
                rise = Some(rise.map_or(c, |r| smax(r, c, k)));
            } else if c < 0.0 {
                sink = Some(sink.map_or(c, |s| smin(s, c, k)));
            }
        };

        for peak in &self.peaks {
            add((peak.height - self.baseline) * profile(peak.center.distance(p), peak.radius, peak.sharpness));
        }
        for line in self.ridges.iter().chain(&self.valleys) {
            let (d, h) = line.closest(p);
            add((h - self.baseline) * profile(d, line.width, line.sharpness));
        }

        self.baseline + rise.unwrap_or(0.0) + sink.unwrap_or(0.0)
    }

    fn flatten(&self, p: Vec2) -> Option<(f32, f32)> {
        self.flats
            .iter()
            .map(|f| {
                let d = f.footprint.distance(local_point(f.origin, f.yaw_degrees, p));
                (1.0 - smoothstep(d / f.falloff.max(1e-3)), f.origin.y)
            })
            .filter(|(w, _)| *w > 0.0)
            .max_by(|a, b| a.0.total_cmp(&b.0))
    }

    fn detail_mask(&self, p: Vec2, shape: f32) -> f32 {
        let mountains = ((shape - self.baseline) / self.max_rise).clamp(0.0, 1.0) * self.settings.elevation_gain;
        let spots: f32 = self.spots.iter().map(|s| s.amount * profile(s.center.distance(p), s.radius, 1.0)).sum();
        (self.settings.ambient_detail + mountains + spots).clamp(0.0, 1.0)
    }
}

impl HeightSource for AuthoredSource {
    fn height(&self, x: f64, z: f64) -> f32 {
        let p = Vec2::new(x as f32, z as f32);
        let shape = self.shape(p);
        let raw = shape + self.detail_mask(p, shape) * self.relief.relief(x, z) * self.settings.relief_scale;
        self.pin(x, z, raw)
    }

    fn pin(&self, x: f64, z: f64, height: f32) -> f32 {
        match self.flatten(Vec2::new(x as f32, z as f32)) {
            Some((weight, target)) => height + (target - height) * weight,
            None => height,
        }
    }

    fn detail_weight(&self, x: f64, z: f64) -> f32 {
        let p = Vec2::new(x as f32, z as f32);
        let flat_weight = self.flatten(p).map_or(0.0, |(w, _)| w);
        self.detail_mask(p, self.shape(p)) * (1.0 - flat_weight)
    }
}

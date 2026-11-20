use flecs_ecs::macros::Component;
use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldDefinition {
    pub name: String,
    pub scene: String,
    pub terrain: WorldTerrain,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldTerrain {
    pub tile_size: f32,
    pub cells: u32,
    pub height_min: f32,
    pub height_max: f32,
    pub seed: u64,
    #[serde(default = "default_baseline")]
    pub baseline: f32,
}

fn default_baseline() -> f32 {
    100.0
}

fn default_sharpness() -> f32 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Anchor {
    Peak {
        id: String,
        position: Vec3,
        radius: f32,
        #[serde(default = "default_sharpness")]
        sharpness: f32,
    },
    Ridge {
        id: String,
        points: Vec<Vec3>,
        width: f32,
        #[serde(default = "default_sharpness")]
        sharpness: f32,
    },
    Valley {
        id: String,
        points: Vec<Vec3>,
        width: f32,
    },
    Plateau {
        id: String,
        position: Vec3,
        #[serde(default)]
        yaw_degrees: f32,
        footprint: Footprint,
        falloff: f32,
    },
    Detail {
        id: String,
        position: Vec3,
        radius: f32,
        amount: f32,
    },
}

#[derive(Component, Debug, Clone, PartialEq)]
pub struct TerrainAnchor(pub Anchor);

impl Anchor {
    pub fn id(&self) -> &str {
        match self {
            Anchor::Peak { id, .. }
            | Anchor::Ridge { id, .. }
            | Anchor::Valley { id, .. }
            | Anchor::Plateau { id, .. }
            | Anchor::Detail { id, .. } => id,
        }
    }
}

#[derive(Component, Debug, Clone, PartialEq)]
pub struct Location {
    pub name: String,
    pub position: Vec3,
    pub yaw_degrees: f32,
    pub footprint: Footprint,
    pub constraints: Constraints,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Footprint {
    Rect { width: f32, depth: f32 },
    Circle { radius: f32 },
}

impl Footprint {
    pub fn contains(&self, local: Vec2) -> bool {
        match self {
            Footprint::Rect { width, depth } => local.x.abs() <= width * 0.5 && local.y.abs() <= depth * 0.5,
            Footprint::Circle { radius } => local.length() <= *radius,
        }
    }

    pub fn distance(&self, local: Vec2) -> f32 {
        match self {
            Footprint::Rect { width, depth } => {
                Vec2::new((local.x.abs() - width * 0.5).max(0.0), (local.y.abs() - depth * 0.5).max(0.0)).length()
            }
            Footprint::Circle { radius } => (local.length() - radius).max(0.0),
        }
    }

    pub fn radius(&self) -> f32 {
        match self {
            Footprint::Rect { width, depth } => Vec2::new(*width, *depth).length() * 0.5,
            Footprint::Circle { radius } => *radius,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Constraints {
    pub max_slope_degrees: Option<f32>,
    pub elevation: Option<Vec2>,
    pub river_within: Option<f32>,
    pub road_access: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorldPath {
    pub name: String,
    pub points: Vec<Vec3>,
    pub width: f32,
}

#[derive(Component, Debug, Clone, PartialEq)]
pub struct Road(pub WorldPath);

#[derive(Component, Debug, Clone, PartialEq)]
pub struct River(pub WorldPath);

impl WorldPath {
    pub fn distance_to(&self, p: Vec2) -> f32 {
        let flat: Vec<Vec2> = self.points.iter().map(|q| Vec2::new(q.x, q.z)).collect();
        match flat.len() {
            0 => f32::INFINITY,
            1 => flat[0].distance(p),
            _ => flat
                .windows(2)
                .map(|w| {
                    let (a, b) = (w[0], w[1]);
                    let ab = b - a;
                    let t = if ab.length_squared() > 0.0 { ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
                    (a + ab * t).distance(p)
                })
                .fold(f32::INFINITY, f32::min),
        }
    }
}

#[derive(Component, Debug, Clone, PartialEq)]
pub struct Landmark {
    pub name: String,
    pub kind: String,
    pub position: Vec3,
}

#[derive(Debug, Error)]
pub enum WorldLoadError {
    #[error("cannot read `{path}`: {source}")]
    Io { path: String, source: std::io::Error },
    #[error("cannot parse `{path}`: {source}")]
    Parse { path: String, source: ron::error::SpannedError },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Violation {
    TooSteep { degrees: f32, max: f32 },
    TooLow { elevation: f32, min: f32 },
    TooHigh { elevation: f32, max: f32 },
    NoRiverNearby { distance: f32, max: f32 },
    NoRoadAccess,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Violation::TooSteep { degrees, max } => write!(f, "slope {degrees:.1}° exceeds {max:.1}°"),
            Violation::TooLow { elevation, min } => write!(f, "elevation {elevation:.0} m is below {min:.0} m"),
            Violation::TooHigh { elevation, max } => write!(f, "elevation {elevation:.0} m is above {max:.0} m"),
            Violation::NoRiverNearby { distance, max } => write!(f, "nearest river is {distance:.0} m away (max {max:.0} m)"),
            Violation::NoRoadAccess => write!(f, "no road reaches the footprint"),
        }
    }
}

impl Location {
    pub fn to_world(&self, local: Vec2) -> Vec2 {
        let (s, c) = self.yaw_degrees.to_radians().sin_cos();
        Vec2::new(self.position.x, self.position.z) + Vec2::new(local.x * c - local.y * s, local.x * s + local.y * c)
    }

    pub fn to_local(&self, world: Vec2) -> Vec2 {
        local_point(self.position, self.yaw_degrees, world)
    }

    pub fn sample_points(&self, step: f32) -> Vec<Vec2> {
        let r = self.footprint.radius();
        let step = step.max(0.5);
        let n = (r / step).ceil() as i32;
        let mut points = Vec::new();
        for j in -n..=n {
            for i in -n..=n {
                let local = Vec2::new(i as f32 * step, j as f32 * step);
                if self.footprint.contains(local) {
                    points.push(self.to_world(local));
                }
            }
        }
        if points.is_empty() {
            points.push(self.to_world(Vec2::ZERO));
        }
        points
    }

    pub fn check(&self, roads: &[WorldPath], rivers: &[WorldPath], step: f32, height: impl Fn(Vec2) -> f32) -> Vec<Violation> {
        let mut violations = Vec::new();
        let c = &self.constraints;
        let center = Vec2::new(self.position.x, self.position.z);

        if let Some(max) = c.max_slope_degrees {
            let delta = step.max(0.5);
            let steepest = self
                .sample_points(step)
                .into_iter()
                .map(|p| {
                    let dx = (height(p + Vec2::X * delta) - height(p - Vec2::X * delta)) / (2.0 * delta);
                    let dz = (height(p + Vec2::Y * delta) - height(p - Vec2::Y * delta)) / (2.0 * delta);
                    (dx * dx + dz * dz).sqrt().atan().to_degrees()
                })
                .fold(0.0, f32::max);
            if steepest > max {
                violations.push(Violation::TooSteep { degrees: steepest, max });
            }
        }

        if let Some(range) = c.elevation {
            let elevation = height(center);
            if elevation < range.x {
                violations.push(Violation::TooLow { elevation, min: range.x });
            } else if elevation > range.y {
                violations.push(Violation::TooHigh { elevation, max: range.y });
            }
        }

        if let Some(max) = c.river_within {
            let distance = rivers.iter().map(|r| r.distance_to(center)).fold(f32::INFINITY, f32::min);
            if distance > max {
                violations.push(Violation::NoRiverNearby { distance, max });
            }
        }

        if c.road_access {
            let reach = self.footprint.radius();
            let reached = roads.iter().any(|r| r.distance_to(center) <= reach + r.width * 0.5);
            if !reached {
                violations.push(Violation::NoRoadAccess);
            }
        }

        violations
    }
}

pub fn local_point(origin: Vec3, yaw_degrees: f32, world: Vec2) -> Vec2 {
    let (s, c) = yaw_degrees.to_radians().sin_cos();
    let d = world - Vec2::new(origin.x, origin.z);
    Vec2::new(d.x * c + d.y * s, -d.x * s + d.y * c)
}

impl WorldDefinition {
    pub fn parse(text: &str) -> Result<WorldDefinition, ron::error::SpannedError> {
        ron::from_str(text)
    }

    pub fn load(path: &str) -> Result<WorldDefinition, WorldLoadError> {
        let text = std::fs::read_to_string(path)
            .map_err(|source| WorldLoadError::Io { path: path.to_owned(), source })?;
        Self::parse(&text).map_err(|source| WorldLoadError::Parse { path: path.to_owned(), source })
    }
}

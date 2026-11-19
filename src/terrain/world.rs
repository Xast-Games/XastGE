use std::collections::HashSet;

use flecs_ecs::macros::Component;
use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldDefinition {
    pub name: String,
    pub terrain: WorldTerrain,
    pub regions: Vec<Region>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub min: Vec2,
    pub max: Vec2,
}

impl Bounds {
    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.min.x && p.y >= self.min.y && p.x <= self.max.x && p.y <= self.max.y
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub id: String,
    pub name: String,
    pub bounds: Bounds,
    #[serde(default)]
    pub locations: Vec<Location>,
    #[serde(default)]
    pub roads: Vec<WorldPath>,
    #[serde(default)]
    pub rivers: Vec<WorldPath>,
    #[serde(default)]
    pub landmarks: Vec<Landmark>,
    #[serde(default)]
    pub anchors: Vec<Anchor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Location {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub scene: Option<String>,
    pub position: Vec3,
    #[serde(default)]
    pub yaw_degrees: f32,
    pub footprint: Footprint,
    #[serde(default)]
    pub constraints: Constraints,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Footprint {
    Rect { width: f32, depth: f32 },
    Circle { radius: f32 },
    Polygon { points: Vec<Vec2> },
}

impl Footprint {
    pub fn contains(&self, local: Vec2) -> bool {
        match self {
            Footprint::Rect { width, depth } => local.x.abs() <= width * 0.5 && local.y.abs() <= depth * 0.5,
            Footprint::Circle { radius } => local.length() <= *radius,
            Footprint::Polygon { points } => {
                let mut inside = false;
                let n = points.len();
                for i in 0..n {
                    let (a, b) = (points[i], points[(i + n - 1) % n]);
                    if (a.y > local.y) != (b.y > local.y)
                        && local.x < (b.x - a.x) * (local.y - a.y) / (b.y - a.y) + a.x
                    {
                        inside = !inside;
                    }
                }
                inside
            }
        }
    }

    pub fn distance(&self, local: Vec2) -> f32 {
        match self {
            Footprint::Rect { width, depth } => {
                Vec2::new((local.x.abs() - width * 0.5).max(0.0), (local.y.abs() - depth * 0.5).max(0.0)).length()
            }
            Footprint::Circle { radius } => (local.length() - radius).max(0.0),
            Footprint::Polygon { points } => {
                if points.len() < 3 || self.contains(local) {
                    return if points.len() < 3 { local.length() } else { 0.0 };
                }
                let n = points.len();
                (0..n)
                    .map(|i| {
                        let (a, b) = (points[i], points[(i + 1) % n]);
                        let ab = b - a;
                        let t = ((local - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
                        (a + ab * t).distance(local)
                    })
                    .fold(f32::INFINITY, f32::min)
            }
        }
    }

    pub fn radius(&self) -> f32 {
        match self {
            Footprint::Rect { width, depth } => Vec2::new(*width, *depth).length() * 0.5,
            Footprint::Circle { radius } => *radius,
            Footprint::Polygon { points } => points.iter().map(|p| p.length()).fold(0.0, f32::max),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Constraints {
    #[serde(default)]
    pub max_slope_degrees: Option<f32>,
    #[serde(default)]
    pub elevation: Option<Vec2>,
    #[serde(default)]
    pub flatten: bool,
    #[serde(default)]
    pub falloff: f32,
    #[serde(default)]
    pub river_within: Option<f32>,
    #[serde(default)]
    pub road_access: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldPath {
    pub id: String,
    pub points: Vec<Vec3>,
    pub width: f32,
    #[serde(default)]
    pub connects: Vec<String>,
}

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LandmarkKind {
    Peak,
    Monument,
    Anomaly,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Landmark {
    pub id: String,
    pub name: String,
    pub kind: LandmarkKind,
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
pub enum WorldIssue {
    DuplicateId(String),
    OutsideRegion { region: String, item: String },
    ShortPath(String),
    EmptyFootprint(String),
    UnknownConnection { path: String, location: String },
}

impl std::fmt::Display for WorldIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorldIssue::DuplicateId(id) => write!(f, "id `{id}` is used more than once"),
            WorldIssue::OutsideRegion { region, item } => write!(f, "`{item}` lies outside region `{region}`"),
            WorldIssue::ShortPath(id) => write!(f, "path `{id}` needs at least two points"),
            WorldIssue::EmptyFootprint(id) => write!(f, "location `{id}` has an empty footprint"),
            WorldIssue::UnknownConnection { path, location } => {
                write!(f, "path `{path}` connects unknown location `{location}`")
            }
        }
    }
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

    pub fn check(&self, region: &Region, step: f32, height: impl Fn(Vec2) -> f32) -> Vec<Violation> {
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
            let distance = region.rivers.iter().map(|r| r.distance_to(center)).fold(f32::INFINITY, f32::min);
            if distance > max {
                violations.push(Violation::NoRiverNearby { distance, max });
            }
        }

        if c.road_access {
            let reach = self.footprint.radius();
            let reached = region.roads.iter().any(|r| r.distance_to(center) <= reach + r.width * 0.5);
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

    pub fn locations(&self) -> impl Iterator<Item = (&Region, &Location)> {
        self.regions.iter().flat_map(|r| r.locations.iter().map(move |l| (r, l)))
    }

    pub fn anchors(&self) -> Vec<Anchor> {
        let anchors = self.regions.iter().flat_map(|r| r.anchors.iter().cloned());
        let flats = self.locations().filter(|(_, l)| l.constraints.flatten).map(|(_, l)| Anchor::Plateau {
            id: l.id.clone(),
            position: l.position,
            yaw_degrees: l.yaw_degrees,
            footprint: l.footprint.clone(),
            falloff: l.constraints.falloff.max(1.0),
        });
        anchors.chain(flats).collect()
    }

    pub fn validate(&self) -> Vec<WorldIssue> {
        let mut issues = Vec::new();
        let mut ids = HashSet::new();
        let mut note = |id: &str, issues: &mut Vec<WorldIssue>| {
            if !ids.insert(id.to_owned()) {
                issues.push(WorldIssue::DuplicateId(id.to_owned()));
            }
        };

        for region in &self.regions {
            note(&region.id, &mut issues);
            let location_ids: HashSet<&str> = region.locations.iter().map(|l| l.id.as_str()).collect();

            for location in &region.locations {
                note(&location.id, &mut issues);
                if !region.bounds.contains(Vec2::new(location.position.x, location.position.z)) {
                    issues.push(WorldIssue::OutsideRegion { region: region.id.clone(), item: location.id.clone() });
                }
                if location.footprint.radius() <= 0.0 {
                    issues.push(WorldIssue::EmptyFootprint(location.id.clone()));
                }
            }

            for path in region.roads.iter().chain(&region.rivers) {
                note(&path.id, &mut issues);
                if path.points.len() < 2 {
                    issues.push(WorldIssue::ShortPath(path.id.clone()));
                }
                for target in &path.connects {
                    if !location_ids.contains(target.as_str()) {
                        issues.push(WorldIssue::UnknownConnection { path: path.id.clone(), location: target.clone() });
                    }
                }
            }

            for anchor in &region.anchors {
                note(anchor.id(), &mut issues);
                if let Anchor::Ridge { id, points, .. } = anchor
                    && points.len() < 2
                {
                    issues.push(WorldIssue::ShortPath(id.clone()));
                }
            }

            for landmark in &region.landmarks {
                note(&landmark.id, &mut issues);
                if !region.bounds.contains(Vec2::new(landmark.position.x, landmark.position.z)) {
                    issues.push(WorldIssue::OutsideRegion { region: region.id.clone(), item: landmark.id.clone() });
                }
            }
        }

        issues
    }
}

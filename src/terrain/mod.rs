pub mod config;
pub mod erosion;
pub mod jobs;
pub mod mesh;
pub mod road;
pub mod shape;
pub mod source;
pub mod splat;
pub mod tile;
pub mod world;

pub use config::{ConfigError, TerrainConfig};
pub use erosion::{BaseJob, BaseMap, BaseRegion, ErodedSource, ErosionSettings};
pub use jobs::{Progress, TileBuild, TileJobs};
pub use mesh::{TerrainVertex, tile_mesh};
pub use shape::{AuthoredSource, ShapeSettings};
pub use road::{ClosestPoint, RoadFrame, RoadProfile, RoadSpline};
pub use source::{FlatSource, HeightSource, LandscapeSettings, LandscapeSource, NoiseSource, SurfaceHints};
pub use splat::{Band, LAYERS, SplatRules, SplatWeights};
pub use tile::{Edge, HeightBounds, PlacedObject, TileCoord, TileData};
pub use world::{
    Anchor, Bounds, Constraints, Footprint, Landmark, LandmarkKind, Location, Region, TerrainAnchor, Violation, WorldDefinition,
    WorldIssue, WorldLoadError, WorldPath, WorldTerrain,
};

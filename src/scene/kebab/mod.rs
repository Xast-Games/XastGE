mod archetypes;
mod builtin;
mod document;
#[cfg(feature = "app")]
mod render;
mod terrain;
mod value;
mod world;

pub use archetypes::{Archetypes, KebabModule, SpawnContext, SpawnIssue, SpawnReport, spawn_scene};
pub use builtin::{
    BoxCollider, DirLight, KebabNodeInfo, KebabSceneRoot, ModelRef, PlayerStart, PointLight, SphereCollider, SpotLight,
    Trigger,
    PREFAB_REF, MODEL_REF, COLLIDER_BOX, COLLIDER_SPHERE, TRIGGER, NO_VOLUME, POINT_LIGHT, DIR_LIGHT, SPOT_LIGHT, SNAP_DISTANCE,
    PLAYER_START,
};
pub use document::{ArchetypeInstance, Greybox, GreyboxShape, KebabNode, KebabScene, SceneError};
#[cfg(feature = "app")]
pub use render::{KebabRenderModule, ModelCache, greybox_mesh, point_light_source, spot_light_source, srgb_to_linear};
pub use terrain::{TERRAIN_NOISE, TERRAIN_PEAK, TERRAIN_PLATEAU, TERRAIN_RIDGE, TERRAIN_VALLEY};
pub use value::{ArchetypeError, color, parse_vec3, vec3};
pub use world::{LANDMARK, LOCATION, RIVER, ROAD};

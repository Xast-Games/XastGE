use std::path::PathBuf;

use flecs_ecs::prelude::*;
use glam::Vec3;
use serde::Deserialize;

use super::{
    archetypes::Archetypes,
    terrain,
    value::{ArchetypeError, color, vec3},
    world,
};

pub const PREFAB_REF:      &str = "prefab_ref";
pub const MODEL_REF:       &str = "model_ref";
pub const COLLIDER_BOX:    &str = "collider_box";
pub const COLLIDER_SPHERE: &str = "collider_sphere";
pub const TRIGGER:         &str = "trigger";
pub const NO_VOLUME:       &str = "no_volume";
pub const POINT_LIGHT:     &str = "point_light";
pub const DIR_LIGHT:       &str = "dir_light";
pub const SPOT_LIGHT:      &str = "spot_light";
pub const SNAP_DISTANCE:   &str = "snap_distance";
pub const PLAYER_START:    &str = "player_start";

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct RefPath {
    pub path: String,
}

#[derive(Component, Debug, Clone, PartialEq)]
pub struct KebabSceneRoot {
    pub path: PathBuf,
}

#[derive(Component, Debug, Clone, PartialEq)]
pub struct KebabNodeInfo {
    pub id: Option<String>,
    pub name: Option<String>,
}

#[derive(Component, Debug, Clone, PartialEq)]
pub struct ModelRef {
    pub path: PathBuf,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct BoxCollider {
    #[serde(deserialize_with = "vec3")]
    pub size: Vec3,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct SphereCollider {
    pub radius: f32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Trigger {
    #[serde(deserialize_with = "vec3")]
    pub size: Vec3,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct PointLight {
    #[serde(deserialize_with = "color")]
    pub color: Vec3,
    pub intensity: f32,
    pub range: f32,
    pub shadows: bool,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct DirLight {
    #[serde(deserialize_with = "color")]
    pub color: Vec3,
    pub intensity: f32,
    pub shadows: bool,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct SpotLight {
    #[serde(deserialize_with = "color")]
    pub color: Vec3,
    pub intensity: f32,
    pub range: f32,
    pub inner_angle: f32,
    pub outer_angle: f32,
    pub shadows: bool,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct PlayerStart {
    pub run_speed: f32,
    pub jump_height: f32,
    pub gravity: f32,
    pub radius: f32,
    pub height: f32,
}

pub(crate) fn register(archetypes: &mut Archetypes) {
    archetypes
        .handler(MODEL_REF, |context, entity, instance| {
            let model = instance.params::<RefPath>()?;
            if model.path.is_empty() {
                return Err(ArchetypeError::Invalid("model_ref has no path".to_owned()));
            }
            entity.set(ModelRef { path: context.path(&model.path) });
            Ok(())
        })
        .component::<BoxCollider>(COLLIDER_BOX)
        .component::<SphereCollider>(COLLIDER_SPHERE)
        .component::<Trigger>(TRIGGER)
        .component::<PointLight>(POINT_LIGHT)
        .component::<DirLight>(DIR_LIGHT)
        .component::<SpotLight>(SPOT_LIGHT)
        .component::<PlayerStart>(PLAYER_START)
        .ignore(NO_VOLUME)
        .ignore(SNAP_DISTANCE);

    terrain::register(archetypes);
    world::register(archetypes);
}

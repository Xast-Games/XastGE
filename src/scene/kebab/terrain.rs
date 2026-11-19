use flecs_ecs::prelude::*;
use glam::{Mat4, Quat, Vec3};
use serde::Deserialize;

use crate::{
    math::Transform,
    terrain::{Anchor, Footprint, Location, TerrainAnchor},
};

use super::{
    archetypes::{Archetypes, SpawnContext},
    document::KebabScene,
    value::{ArchetypeError, vec3},
};

pub const TERRAIN_PEAK:    &str = "terrain_peak";
pub const TERRAIN_RIDGE:   &str = "terrain_ridge";
pub const TERRAIN_VALLEY:  &str = "terrain_valley";
pub const TERRAIN_PLATEAU: &str = "terrain_plateau";
pub const TERRAIN_NOISE:   &str = "terrain_noise";

#[derive(Deserialize)]
struct Peak {
    radius: f32,
    sharpness: f32,
}

#[derive(Deserialize)]
struct Link {
    next: Option<String>,
}

#[derive(Deserialize)]
struct Ridge {
    width: f32,
    sharpness: f32,
}

#[derive(Deserialize)]
struct Valley {
    width: f32,
}

#[derive(Deserialize)]
struct Plateau {
    #[serde(deserialize_with = "vec3")]
    size: Vec3,
    falloff: f32,
}

#[derive(Deserialize)]
struct Noise {
    radius: f32,
    amount: f32,
}

pub fn location_transform(location: &Location) -> Transform {
    Transform {
        translation: location.position,
        rotation: Quat::from_rotation_y(-location.yaw_degrees.to_radians()),
        scale: Vec3::ONE,
    }
}

struct Placed {
    position: Vec3,
    scale: f32,
    yaw_degrees: f32,
}

fn placed(world: Mat4) -> Placed {
    let x = world.transform_vector3(Vec3::X);
    Placed {
        position: world.transform_point3(Vec3::ZERO),
        scale: x.length(),
        yaw_degrees: x.z.atan2(x.x).to_degrees(),
    }
}

fn attach(entity: EntityView<'_>, anchor: Anchor) {
    entity.world().entity().child_of(entity).set(TerrainAnchor(anchor));
}

fn next(scene: &KebabScene, index: usize, archetype: &str) -> Result<Option<usize>, ArchetypeError> {
    let Some(instance) = scene.nodes[index].archetype(archetype) else {
        return Ok(None);
    };
    match instance.params::<Link>()?.next {
        None => Ok(None),
        Some(id) => scene
            .find(&id)
            .map(Some)
            .ok_or_else(|| ArchetypeError::Invalid(format!("`next` points to a missing node `{id}`"))),
    }
}

fn walk(scene: &KebabScene, start: usize, archetype: &str) -> Result<(Vec<usize>, bool), ArchetypeError> {
    let mut order = Vec::new();
    let mut current = Some(start);
    while let Some(index) = current {
        if order.contains(&index) {
            return Ok((order, true));
        }
        order.push(index);
        current = next(scene, index, archetype)?;
    }
    Ok((order, false))
}

fn is_linked_to(scene: &KebabScene, index: usize, archetype: &str) -> bool {
    (0..scene.nodes.len()).any(|other| next(scene, other, archetype).ok().flatten() == Some(index))
}

fn chain(context: &SpawnContext<'_>, archetype: &str) -> Result<Option<Vec<Vec3>>, ArchetypeError> {
    let scene = context.scene;

    if is_linked_to(scene, context.index, archetype) {
        return match walk(scene, context.index, archetype) {
            Ok((order, true)) if order.first() == Some(&context.index) => {
                Err(ArchetypeError::Invalid(format!("`{archetype}` chain has no start, it is a closed loop")))
            }
            _ => Ok(None),
        };
    }

    let (order, looped) = walk(scene, context.index, archetype)?;
    if looped {
        return Err(ArchetypeError::Invalid(format!("`{archetype}` chain loops back")));
    }

    let points = order.iter().map(|&i| placed(context.world_matrix(&scene.nodes[i])).position).collect();
    Ok(Some(points))
}

pub(crate) fn register(archetypes: &mut Archetypes) {
    archetypes
        .handler(TERRAIN_PEAK, |context, entity, instance| {
            let peak = instance.params::<Peak>()?;
            let at = placed(context.world_matrix(context.node));
            attach(entity, Anchor::Peak {
                id: context.node.label(),
                position: at.position,
                radius: peak.radius * at.scale,
                sharpness: peak.sharpness,
            });
            Ok(())
        })
        .handler(TERRAIN_PLATEAU, |context, entity, instance| {
            let plateau = instance.params::<Plateau>()?;
            let at = placed(context.world_matrix(context.node));
            let size = plateau.size * at.scale;
            attach(entity, Anchor::Plateau {
                id: context.node.label(),
                position: at.position,
                yaw_degrees: at.yaw_degrees,
                footprint: Footprint::Rect { width: size.x, depth: size.z },
                falloff: plateau.falloff * at.scale,
            });
            Ok(())
        })
        .handler(TERRAIN_NOISE, |context, entity, instance| {
            let noise = instance.params::<Noise>()?;
            let at = placed(context.world_matrix(context.node));
            attach(entity, Anchor::Detail {
                id: context.node.label(),
                position: at.position,
                radius: noise.radius * at.scale,
                amount: noise.amount,
            });
            Ok(())
        })
        .handler(TERRAIN_RIDGE, |context, entity, instance| {
            let ridge = instance.params::<Ridge>()?;
            if let Some(points) = chain(context, TERRAIN_RIDGE)? {
                let scale = placed(context.world_matrix(context.node)).scale;
                attach(entity, Anchor::Ridge {
                    id: context.node.label(),
                    points,
                    width: ridge.width * scale,
                    sharpness: ridge.sharpness,
                });
            }
            Ok(())
        })
        .handler(TERRAIN_VALLEY, |context, entity, instance| {
            let valley = instance.params::<Valley>()?;
            if let Some(points) = chain(context, TERRAIN_VALLEY)? {
                let scale = placed(context.world_matrix(context.node)).scale;
                attach(entity, Anchor::Valley {
                    id: context.node.label(),
                    points,
                    width: valley.width * scale,
                });
            }
            Ok(())
        });
}

use flecs_ecs::prelude::*;
use glam::Vec2;
use serde::Deserialize;

use crate::terrain::{Constraints, Landmark, Location, River, Road, WorldPath};

use super::{
    archetypes::{Archetypes, SpawnContext},
    terrain::{chain, footprint, placed},
    value::ArchetypeError,
};

pub const LOCATION: &str = "location";
pub const ROAD:     &str = "road";
pub const RIVER:    &str = "river";
pub const LANDMARK: &str = "landmark";

#[derive(Deserialize)]
struct LocationParams {
    max_slope_degrees: f32,
    min_elevation: f32,
    max_elevation: f32,
    river_within: f32,
    road_access: bool,
}

impl LocationParams {
    fn constraints(&self) -> Constraints {
        Constraints {
            max_slope_degrees: (self.max_slope_degrees > 0.0).then_some(self.max_slope_degrees),
            elevation: (self.max_elevation > self.min_elevation).then(|| Vec2::new(self.min_elevation, self.max_elevation)),
            river_within: (self.river_within > 0.0).then_some(self.river_within),
            road_access: self.road_access,
        }
    }
}

#[derive(Deserialize)]
struct PathParams {
    width: f32,
}

#[derive(Deserialize)]
struct LandmarkParams {
    kind: String,
}

fn path(context: &SpawnContext<'_>, archetype: &str, width: f32) -> Result<Option<WorldPath>, ArchetypeError> {
    let Some(points) = chain(context, archetype)? else {
        return Ok(None);
    };
    let scale = placed(context.world_matrix(context.node)).scale;
    Ok(Some(WorldPath { name: context.node.label(), points, width: width * scale }))
}

pub(crate) fn register(archetypes: &mut Archetypes) {
    archetypes
        .handler(LOCATION, |context, entity, instance| {
            let params = instance.params::<LocationParams>()?;
            let (at, footprint) = footprint(context, LOCATION)?;
            entity.set(Location {
                name: context.node.label(),
                position: at.position,
                yaw_degrees: at.yaw_degrees,
                footprint,
                constraints: params.constraints(),
            });
            Ok(())
        })
        .handler(ROAD, |context, entity, instance| {
            let params = instance.params::<PathParams>()?;
            if let Some(road) = path(context, ROAD, params.width)? {
                entity.set(Road(road));
            }
            Ok(())
        })
        .handler(RIVER, |context, entity, instance| {
            let params = instance.params::<PathParams>()?;
            if let Some(river) = path(context, RIVER, params.width)? {
                entity.set(River(river));
            }
            Ok(())
        })
        .handler(LANDMARK, |context, entity, instance| {
            let params = instance.params::<LandmarkParams>()?;
            entity.set(Landmark {
                name: context.node.label(),
                kind: params.kind,
                position: placed(context.world_matrix(context.node)).position,
            });
            Ok(())
        });
}

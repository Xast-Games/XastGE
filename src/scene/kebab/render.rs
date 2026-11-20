use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use flecs_ecs::prelude::*;
use glam::Vec4;

use crate::{
    Render,
    app::RenderSlot,
    mesh::{Mesh, Vertex},
    render::{
        RenderDevice,
        lit::{Cone, LightSource, LitMaterial, LitParams},
        model::{Model, ModelData, ModelPart},
        registry::{RenderRegistry, RenderRegistryModule},
        updated,
    },
};

use super::{
    builtin::{ModelRef, PointLight, SpotLight},
    document::{Greybox, GreyboxShape},
};

#[derive(Component, Default)]
pub struct ModelCache {
    models: HashMap<PathBuf, Option<Model>>,
}

impl ModelCache {
    pub fn get(&mut self, path: &Path, render_device: &mut RenderDevice, registry: &mut RenderRegistry) -> Option<Model> {
        self.models
            .entry(path.to_owned())
            .or_insert_with(|| match ModelData::load(path) {
                Ok(data) => Some(Model::upload(&data, render_device, registry)),
                Err(e) => {
                    log::error!("{e}");
                    None
                }
            })
            .clone()
    }
}

pub fn srgb_to_linear(color: Vec4) -> Vec4 {
    let channel = |c: f32| {
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    Vec4::new(channel(color.x), channel(color.y), channel(color.z), color.w)
}

pub fn greybox_mesh(greybox: &Greybox) -> Mesh<Vertex> {
    match greybox.shape {
        GreyboxShape::Box => Mesh::cuboid(greybox.size),
        GreyboxShape::Sphere => Mesh::ellipsoid(greybox.size, 24, 16),
    }
}

fn greybox_model(greybox: &Greybox, render_device: &mut RenderDevice, registry: &mut RenderRegistry) -> Model {
    let albedo = registry.white_texture(render_device);
    let params = LitParams::new(srgb_to_linear(greybox.color), 0.9);
    let material = LitMaterial::new(params, albedo, render_device, registry);
    let mesh = updated(greybox_mesh(greybox), render_device, registry);
    Model::new(vec![ModelPart { mesh, material }])
}

pub fn point_light_source(light: &PointLight) -> LightSource {
    LightSource { color: light.color, intensity: light.intensity, range: light.range, cone: None }
}

pub fn spot_light_source(light: &SpotLight) -> LightSource {
    LightSource {
        color: light.color,
        intensity: light.intensity,
        range: light.range,
        cone: Some(Cone { inner_degrees: light.inner_angle, outer_degrees: light.outer_angle }),
    }
}

#[derive(Component)]
pub struct KebabRenderModule;

impl Module for KebabRenderModule {
    fn module(world: &World) {
        world.import::<RenderRegistryModule>();

        world.component::<ModelCache>().add_trait::<flecs::Singleton>();
        world.set(ModelCache::default());

        let models = world.query::<&ModelRef>().without(Model::id()).build();
        let greyboxes = world.query::<&Greybox>().without(Model::id()).without(ModelRef::id()).build();
        let points = world.query::<&PointLight>().without(LightSource::id()).build();
        let spots = world.query::<&SpotLight>().without(LightSource::id()).build();

        world.system::<(&mut RenderSlot, &mut RenderRegistry, &mut ModelCache)>()
            .kind(Render)
            .each(move |(slot, registry, cache)| {
                let device = slot.device_mut();

                models.each_entity(|entity, model_ref| {
                    if let Some(model) = cache.get(&model_ref.path, device, registry) {
                        entity.set(model);
                    }
                });
                greyboxes.each_entity(|entity, greybox| {
                    entity.set(greybox_model(greybox, device, registry));
                });
                points.each_entity(|entity, light| {
                    entity.set(point_light_source(light));
                });
                spots.each_entity(|entity, light| {
                    entity.set(spot_light_source(light));
                });
            });
    }
}

mod mesh;

#[cfg(feature = "render")]
mod render {
    mod lit;
    mod model;
    #[cfg(feature = "app")]
    mod terrain_render;
    mod texture;
}

#[cfg(feature = "scene")]
mod scene {
    mod kebab;
}

mod terrain {
    mod config;
    mod erosion;
    mod jobs;
    mod mesh;
    mod road;
    mod shape;
    mod source;
    mod splat;
    mod tile;
    mod world;
}

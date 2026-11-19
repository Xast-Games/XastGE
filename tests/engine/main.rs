mod mesh;

#[cfg(feature = "render")]
mod render {
    #[cfg(feature = "app")]
    mod terrain_render;
    mod texture;
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

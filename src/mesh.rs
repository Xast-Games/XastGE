use bytemuck::{Pod, Zeroable};
use flecs_ecs::macros::Component;
use glam::{Vec2, Vec3, Vec4};

#[cfg(feature = "render")]
use crate::{render::{Drawable, RenderDevice, buffer::BufferHandle, registry::RenderRegistry, types::*}};

#[derive(Pod, Zeroable, Clone, Copy, Debug)]
#[repr(C)]
pub struct Vertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub texcoord: Vec2,
}

#[derive(Pod, Zeroable, Clone, Copy, Debug)]
#[repr(C)]
pub struct DebugLineVertex {
    pub position: Vec3,
    pub color: Vec3,
}

#[derive(Pod, Zeroable, Clone, Copy, Debug)]
#[repr(C)]
pub struct SkinnedVertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub texcoord: Vec2,
    pub joint_indices: Vec4,
    pub joint_weights: Vec4,
}

impl SkinnedVertex {
    pub fn from_vertex(vertex: Vertex) -> Self {
        Self {
            position: vertex.position,
            normal: vertex.normal,
            texcoord: vertex.texcoord,
            joint_indices: Vec4::ZERO,
            joint_weights: Vec4::new(1.0, 0.0, 0.0, 0.0),
        }
    }
}

// GPU vertex buffer layouts: these only make sense when there is a render device to feed them to.
#[cfg(feature = "render")]
mod gpu {
    use super::*;

    impl DebugLineVertex {
        const ATTRIBS: &[wgpu::VertexAttribute] = &wgpu::vertex_attr_array![
            0 => Float32x3,
            1 => Float32x3,
        ];

        pub fn vertex_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<DebugLineVertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: Self::ATTRIBS,
            }
        }
    }

    // TODO: VertexTrait
    pub trait VertexTrait: Pod + 'static + Send + Sync {}
    impl VertexTrait for Vertex {}
    impl VertexTrait for DebugLineVertex {}

    impl Vertex {
        const ATTRIBS: &[wgpu::VertexAttribute] = &wgpu::vertex_attr_array![
            0 => Float32x3,
            1 => Float32x3,
            2 => Float32x2,
        ];

        pub fn vertex_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: Self::ATTRIBS,
            }
        }
    }

    impl SkinnedVertex {
        const ATTRIBS: &[wgpu::VertexAttribute] = &wgpu::vertex_attr_array![
            0 => Float32x3,
            1 => Float32x3,
            2 => Float32x2,
            3 => Float32x4,
            4 => Float32x4,
        ];

        pub fn vertex_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<SkinnedVertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: Self::ATTRIBS,
            }
        }
    }
}

#[cfg(feature = "render")]
pub use gpu::VertexTrait;

#[derive(Debug, Component, Clone)]
pub struct Mesh<V: Send + Sync + 'static> {
    pub vertices: Vec<V>,
    pub indices: Vec<u32>,

    #[cfg(feature = "render")]
    pub vertex_buffer: Option<BufferHandle>,
    #[cfg(feature = "render")]
    pub index_buffer: Option<BufferHandle>,
}

impl<V: Send + Sync + 'static> Default for Mesh<V> {
    fn default() -> Self {
        Mesh {
            vertices: vec![],
            indices: vec![],
            #[cfg(feature = "render")]
            vertex_buffer: None,
            #[cfg(feature = "render")]
            index_buffer: None,
        }
    }
}

impl<V: Send + Sync + 'static> Mesh<V> {
    pub fn new(vertices: Vec<V>, indices: Vec<u32>) -> Mesh<V> {
        Mesh {
            vertices,
            indices,
            #[cfg(feature = "render")]
            vertex_buffer: None,
            #[cfg(feature = "render")]
            index_buffer: None,
        }
    }
}

impl Mesh<Vertex> {
    pub fn plane(size: f32, cells: u32) -> Self {
        let cells = cells.max(1);
        let stride = cells + 1;
        let step = size / cells as f32;
        let half = size * 0.5;

        let mut vertices = Vec::with_capacity((stride * stride) as usize);
        for z in 0..stride {
            for x in 0..stride {
                vertices.push(Vertex {
                    position: Vec3::new(x as f32 * step - half, 0.0, z as f32 * step - half),
                    normal: Vec3::Y,
                    texcoord: Vec2::new(x as f32 / cells as f32, z as f32 / cells as f32),
                });
            }
        }

        let mut indices = Vec::with_capacity((cells * cells * 6) as usize);
        for z in 0..cells {
            for x in 0..cells {
                let i00 = z * stride + x;
                let i10 = i00 + 1;
                let i01 = i00 + stride;
                let i11 = i01 + 1;
                indices.extend_from_slice(&[i00, i01, i10, i10, i01, i11]);
            }
        }

        Mesh::new(vertices, indices)
    }

    pub fn cuboid(size: Vec3) -> Self {
        let half = size * 0.5;
        let faces = [
            (Vec3::X,     Vec3::NEG_Z, Vec3::Y),
            (Vec3::NEG_X, Vec3::Z,     Vec3::Y),
            (Vec3::Y,     Vec3::X,     Vec3::NEG_Z),
            (Vec3::NEG_Y, Vec3::X,     Vec3::Z),
            (Vec3::Z,     Vec3::X,     Vec3::Y),
            (Vec3::NEG_Z, Vec3::NEG_X, Vec3::Y),
        ];
        let corners = [(-1.0, -1.0, 0.0, 1.0), (1.0, -1.0, 1.0, 1.0), (1.0, 1.0, 1.0, 0.0), (-1.0, 1.0, 0.0, 0.0)];

        let mut vertices = Vec::with_capacity(24);
        let mut indices = Vec::with_capacity(36);
        for (normal, u, v) in faces {
            let base = vertices.len() as u32;
            for (a, b, s, t) in corners {
                vertices.push(Vertex {
                    position: (normal + u * a + v * b) * half,
                    normal,
                    texcoord: Vec2::new(s, t),
                });
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }

        Mesh::new(vertices, indices)
    }

    pub fn ellipsoid(size: Vec3, segments: u32, rings: u32) -> Self {
        let segments = segments.max(3);
        let rings = rings.max(2);
        let radii = size * 0.5;
        let stride = segments + 1;

        let mut vertices = Vec::with_capacity((stride * (rings + 1)) as usize);
        for ring in 0..=rings {
            let theta = std::f32::consts::PI * ring as f32 / rings as f32;
            for segment in 0..=segments {
                let phi = std::f32::consts::TAU * segment as f32 / segments as f32;
                let unit = Vec3::new(theta.sin() * phi.cos(), theta.cos(), theta.sin() * phi.sin());
                vertices.push(Vertex {
                    position: unit * radii,
                    normal: (unit / radii).normalize_or(unit),
                    texcoord: Vec2::new(segment as f32 / segments as f32, ring as f32 / rings as f32),
                });
            }
        }

        let mut indices = Vec::with_capacity((segments * rings * 6) as usize);
        for ring in 0..rings {
            for segment in 0..segments {
                let a = ring * stride + segment;
                let b = a + stride;
                indices.extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]);
            }
        }

        Mesh::new(vertices, indices)
    }
}

#[cfg(feature = "render")]
impl Mesh<Vertex> {
    pub fn load_obj(path: &str) -> Self {
        let (mut models, _) = tobj::load_obj(
            path,
            &tobj::LoadOptions {
                single_index: true,
                triangulate: true,
                ignore_points: true,
                ..Default::default()
            },
        ).expect("Cannot load OBJ file");

        let m = models.swap_remove(0);

        let mut vertices = Vec::<Vertex>::new();
        let indices = m.mesh.indices;

        for i in 0..m.mesh.positions.len() / 3 {
            let texcoord: Vec2;

            let position = Vec3::new(
                m.mesh.positions[i*3],
                m.mesh.positions[i*3+1],
                m.mesh.positions[i*3+2],
            );

            let normal = Vec3::new(
                m.mesh.normals[i*3],
                m.mesh.normals[i*3+1],
                m.mesh.normals[i*3+2],
            );

            if i*2 < m.mesh.texcoords.len() {
                texcoord = Vec2::new(
                    m.mesh.texcoords[i*2],
                    m.mesh.texcoords[i*2+1],
                );
            } else {
                texcoord = Vec2::ZERO;
            }

            vertices.push(Vertex {
                position,
                normal,
                texcoord,
            });
        }

        Mesh {
            vertices,
            indices,

            vertex_buffer: None,
            index_buffer: None,
        }
    }
}

#[cfg(feature = "render")]
impl<V: Send + Sync + 'static> Mesh<V> {
    pub fn release(&mut self, registry: &mut RenderRegistry) {
        if let Some(buffer) = self.vertex_buffer.take() {
            registry.remove_buffer(buffer);
        }
        if let Some(buffer) = self.index_buffer.take() {
            registry.remove_buffer(buffer);
        }
    }
}

#[cfg(feature = "render")]
impl<V: Pod + Send + Sync + 'static> Drawable for Mesh<V> {
    fn update(&mut self, render_device: &mut RenderDevice, registry: &mut RenderRegistry) {
        if self.vertex_buffer.is_none() {
            self.vertex_buffer = Some(
                registry.new_buffer::<V>(render_device, self.vertices.len(), BufferUsages::VERTEX)
            );
        }

        if self.index_buffer.is_none() && !self.indices.is_empty() {
            self.index_buffer = Some(
                registry.new_buffer::<u32>(render_device, self.indices.len(), BufferUsages::INDEX)
            );
        }

        let Some(vb) = self.vertex_buffer else { unreachable!() };

        registry
            .get_buffer_mut(vb)
            .expect("Cannot call update() on Mesh")
            .fill(render_device, 0, &self.vertices);

        if let Some(ib) = self.index_buffer {
            registry
                .get_buffer_mut(ib)
                .expect("Cannot call update() on Mesh")
                .fill(render_device, 0, &self.indices);
        }
    }

    fn vertex_buffer(&self) -> BufferHandle {
        self.vertex_buffer
            .expect("Mesh is not set up with update()")
    }

    fn index_buffer(&self) -> Option<BufferHandle> {
        match self.indices.len() {
            0 => None,
            _ => Some(
                self.index_buffer
                    .expect("Mesh is not set up with update()")
            ),
        }
    }

    fn indices(&self) -> u32 {
        self.indices.len() as u32
    }
}

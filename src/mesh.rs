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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plane_faces_up_and_is_centred() {
        let plane = Mesh::<Vertex>::plane(10.0, 4);

        assert_eq!(plane.vertices.len(), 25);
        assert_eq!(plane.indices.len(), 4 * 4 * 6);
        assert_eq!(plane.vertices[0].position, Vec3::new(-5.0, 0.0, -5.0));
        assert_eq!(plane.vertices[24].position, Vec3::new(5.0, 0.0, 5.0));

        for tri in plane.indices.chunks(3) {
            let a = plane.vertices[tri[0] as usize].position;
            let b = plane.vertices[tri[1] as usize].position;
            let c = plane.vertices[tri[2] as usize].position;
            assert!((b - a).cross(c - a).y > 0.0);
        }
    }
}

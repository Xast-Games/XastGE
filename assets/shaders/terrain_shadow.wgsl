struct Light {
    view_proj: mat4x4<f32>,
    sun: vec4<f32>,
    params: vec4<f32>,
}

@group(0) @binding(0) var<uniform> light: Light;

var<push_constant> transform: mat4x4<f32>;

struct VertexInput {
    @location(0) position: vec3<f32>,
}

@vertex
fn vertex(input: VertexInput) -> @builtin(position) vec4<f32> {
    return light.view_proj * transform * vec4<f32>(input.position, 1.0);
}

@fragment
fn fragment() {
}

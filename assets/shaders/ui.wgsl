@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var tex_sampler: sampler;

struct Push {
    viewport: vec2<f32>,
}

var<push_constant> push: Push;

const MODE_GLYPH: u32 = 1u;

struct VertexInput {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) mode: u32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) mode: u32,
}

@vertex
fn vertex(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let ndc = vec2<f32>(
        input.pos.x / push.viewport.x * 2.0 - 1.0,
        1.0 - input.pos.y / push.viewport.y * 2.0,
    );

    out.clip_position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = input.uv;
    out.color = input.color;
    out.mode = input.mode;

    return out;
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(tex, tex_sampler, input.uv);

    if input.mode == MODE_GLYPH {
        // R8 atlas: red is coverage
        return vec4<f32>(input.color.rgb, input.color.a * texel.r);
    }

    return input.color * texel;
}

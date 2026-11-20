struct Params {
    base_color: vec4<f32>,
    roughness: f32,
    specular: f32,
    padding: vec2<f32>,
}

struct CameraUniform {
    position: vec3<f32>,
    view_proj: mat4x4<f32>,
}

struct Light {
    view_proj: mat4x4<f32>,
    sun: vec4<f32>,
    params: vec4<f32>,
}

struct PointLight {
    position: vec4<f32>,
    color: vec4<f32>,
    direction: vec4<f32>,
    cone: vec4<f32>,
}

struct Lights {
    sky: vec4<f32>,
    ground: vec4<f32>,
    fog: vec4<f32>,
    count: vec4<u32>,
    items: array<PointLight, 32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var albedo_tex: texture_2d<f32>;
@group(0) @binding(2) var albedo_sampler: sampler;
@group(1) @binding(0) var<uniform> cam_uniform: CameraUniform;
@group(2) @binding(0) var<uniform> light: Light;
@group(2) @binding(1) var<uniform> lights: Lights;
@group(3) @binding(0) var shadow_map: texture_depth_2d;

var<push_constant> transform: mat4x4<f32>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) texcoord: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) texcoord: vec2<f32>,
}

@vertex
fn vertex(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world = transform * vec4<f32>(input.position, 1.0);

    out.world = world.xyz;
    out.normal = (transform * vec4<f32>(input.normal, 0.0)).xyz;
    out.texcoord = input.texcoord;
    out.clip_position = cam_uniform.view_proj * world;

    return out;
}

fn shadow_factor(world: vec3<f32>, n: vec3<f32>) -> f32 {
    if light.params.w < 0.5 {
        return 1.0;
    }

    let clip = light.view_proj * vec4<f32>(world + n * light.params.z, 1.0);
    let ndc = clip.xyz / clip.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    if uv.x < 0.0 || uv.y < 0.0 || uv.x > 1.0 || uv.y > 1.0 || ndc.z > 1.0 {
        return 1.0;
    }

    let size = vec2<i32>(textureDimensions(shadow_map));
    let center = vec2<i32>(uv * vec2<f32>(size));
    let depth = ndc.z - light.params.y;

    var lit = 0.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let texel = clamp(center + vec2<i32>(x, y), vec2<i32>(0), size - vec2<i32>(1));
            let stored = textureLoad(shadow_map, texel, 0);
            lit = lit + select(0.0, 1.0, depth <= stored);
        }
    }
    return lit / 9.0;
}

fn blinn(n: vec3<f32>, l: vec3<f32>, v: vec3<f32>) -> f32 {
    let h = normalize(l + v);
    let alpha = params.roughness * params.roughness;
    let shininess = clamp(2.0 / max(alpha * alpha, 1e-4) - 2.0, 1.0, 2048.0);
    let fresnel = 0.04 + 0.96 * pow(1.0 - max(dot(h, v), 0.0), 5.0);
    return pow(max(dot(n, h), 0.0), shininess) * (shininess + 8.0) / 25.13 * fresnel * params.specular;
}

fn local_light(index: u32, world: vec3<f32>, n: vec3<f32>, v: vec3<f32>, albedo: vec3<f32>) -> vec3<f32> {
    let item = lights.items[index];
    let offset = item.position.xyz - world;
    let dist = length(offset);
    let reach = item.position.w;
    if dist >= reach || dist <= 1e-4 {
        return vec3<f32>(0.0);
    }

    let l = offset / dist;
    let fade = clamp(1.0 - pow(dist / reach, 4.0), 0.0, 1.0);
    var attenuation = fade * fade / max(dist * dist, 0.01);

    if item.cone.z > 0.5 {
        let along = dot(-l, normalize(item.direction.xyz));
        attenuation = attenuation * smoothstep(item.cone.y, item.cone.x, along);
    }

    let n_dot_l = max(dot(n, l), 0.0);
    let radiance = item.color.rgb * item.color.w * attenuation;
    return radiance * n_dot_l * (albedo + vec3<f32>(blinn(n, l, v)));
}

@fragment
fn fragment(input: VertexOutput, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let world = input.world;
    let facing = normalize(input.normal);
    let n = select(-facing, facing, front);
    let v = normalize(cam_uniform.position - world);

    let albedo = (textureSample(albedo_tex, albedo_sampler, input.texcoord) * params.base_color).rgb;

    let sun_dir = normalize(light.sun.xyz);
    let sun_color = vec3<f32>(1.0, 0.95, 0.85) * light.sun.w;
    let n_dot_l = max(dot(n, sun_dir), 0.0);
    let shadow = shadow_factor(world, n);
    let sun = sun_color * n_dot_l * shadow;

    let ambient = mix(lights.ground.rgb, lights.sky.rgb, n.y * 0.5 + 0.5) * lights.sky.w;

    var color = albedo * (sun * 0.9 + ambient) + sun * blinn(n, sun_dir, v);

    let count = min(lights.count.x, 32u);
    for (var i = 0u; i < count; i = i + 1u) {
        color = color + local_light(i, world, n, v, albedo);
    }

    let view_distance = length(world - cam_uniform.position);
    let fog = 1.0 - exp(-view_distance * lights.fog.w);
    color = mix(color, lights.fog.rgb, clamp(fog, 0.0, 1.0));

    return vec4<f32>(color, 1.0);
}

struct Palette {
    grass: vec4<f32>,
    soil: vec4<f32>,
    rocky: vec4<f32>,
    cliff: vec4<f32>,
    sky: vec4<f32>,
    sharpness: f32,
    macro_strength: f32,
    macro_wavelength: f32,
    fog_density: f32,
    texture_scale: f32,
    border_noise: f32,
    height_blend: f32,
    detail_distance: f32,
    far_distance: f32,
    specular: f32,
    stochastic: f32,
    moisture_strength: f32,
}

struct Light {
    view_proj: mat4x4<f32>,
    sun: vec4<f32>,
    params: vec4<f32>,
}

struct CameraUniform {
    position: vec3<f32>,
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> palette: Palette;
@group(0) @binding(1) var albedo_tex: texture_2d_array<f32>;
@group(0) @binding(2) var albedo_sampler: sampler;
@group(0) @binding(3) var normal_tex: texture_2d_array<f32>;
@group(0) @binding(4) var normal_sampler: sampler;
@group(1) @binding(0) var<uniform> cam_uniform: CameraUniform;
@group(2) @binding(0) var<uniform> light: Light;
@group(3) @binding(0) var shadow_map: texture_depth_2d;

var<push_constant> transform: mat4x4<f32>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) splat: vec4<f32>,
    @location(3) hints: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) splat: vec4<f32>,
    @location(3) hints: vec4<f32>,
}

@vertex
fn vertex(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world = transform * vec4<f32>(input.position, 1.0);

    out.world = world.xyz;
    out.normal = (transform * vec4<f32>(input.normal, 0.0)).xyz;
    out.splat = input.splat;
    out.hints = input.hints;
    out.clip_position = cam_uniform.view_proj * world;

    return out;
}

fn hash(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(0.1031, 0.1030));
    let r = q + dot(q, q.yx + 33.33);
    return fract((r.x + r.y) * r.x);
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);

    let a = hash(i);
    let b = hash(i + vec2<f32>(1.0, 0.0));
    let c = hash(i + vec2<f32>(0.0, 1.0));
    let d = hash(i + vec2<f32>(1.0, 1.0));

    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var sum = 0.0;
    var amplitude = 0.5;
    var q = p;
    for (var i = 0; i < 4; i = i + 1) {
        sum = sum + value_noise(q) * amplitude;
        q = q * 2.03 + vec2<f32>(17.0, 31.0);
        amplitude = amplitude * 0.5;
    }
    return sum / 0.9375;
}

fn hash2(p: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(hash(p), hash(p + vec2<f32>(19.19, 47.47)));
}

struct Surface {
    albedo: vec3<f32>,
    height: f32,
    normal: vec3<f32>,
    occlusion: f32,
    roughness: f32,
}

struct Texel {
    albedo: vec4<f32>,
    detail: vec4<f32>,
}

struct Grid {
    weights: vec3<f32>,
    a: vec2<f32>,
    b: vec2<f32>,
    c: vec2<f32>,
}

fn triangle_grid(uv: vec2<f32>) -> Grid {
    let skewed = vec2<f32>(uv.x - uv.y * 0.57735027, uv.y * 1.15470054) * 1.2;
    let base = floor(skewed);
    let f = fract(skewed);
    let z = 1.0 - f.x - f.y;

    var g: Grid;
    if z > 0.0 {
        g.weights = vec3<f32>(z, f.y, f.x);
        g.a = base;
        g.b = base + vec2<f32>(0.0, 1.0);
        g.c = base + vec2<f32>(1.0, 0.0);
    } else {
        g.weights = vec3<f32>(-z, 1.0 - f.y, 1.0 - f.x);
        g.a = base + vec2<f32>(1.0, 1.0);
        g.b = base + vec2<f32>(1.0, 0.0);
        g.c = base + vec2<f32>(0.0, 1.0);
    }
    return g;
}

fn fetch(layer: i32, uv: vec2<f32>, gx: vec2<f32>, gy: vec2<f32>) -> Texel {
    var t: Texel;
    t.albedo = textureSampleGrad(albedo_tex, albedo_sampler, uv, layer, gx, gy);
    t.detail = textureSampleGrad(normal_tex, normal_sampler, uv, layer, gx, gy);
    return t;
}

fn fetch_tiled(layer: i32, uv: vec2<f32>, gx: vec2<f32>, gy: vec2<f32>) -> Texel {
    if palette.stochastic < 0.5 {
        return fetch(layer, uv, gx, gy);
    }

    let g = triangle_grid(uv);
    var w = pow(g.weights, vec3<f32>(4.0));
    w = w / (w.x + w.y + w.z);

    let a = fetch(layer, uv + hash2(g.a), gx, gy);
    let b = fetch(layer, uv + hash2(g.b), gx, gy);
    let c = fetch(layer, uv + hash2(g.c), gx, gy);

    var t: Texel;
    t.albedo = a.albedo * w.x + b.albedo * w.y + c.albedo * w.z;
    t.detail = a.detail * w.x + b.detail * w.y + c.detail * w.z;
    return t;
}

fn unpack(t: vec4<f32>) -> vec3<f32> {
    let xy = t.xy * 2.0 - 1.0;
    return vec3<f32>(xy, sqrt(max(1.0 - dot(xy, xy), 0.0)));
}

fn planar(layer: i32, p: vec3<f32>, n: vec3<f32>, gx: vec3<f32>, gy: vec3<f32>) -> Surface {
    let t = fetch_tiled(layer, p.xz, gx.xz, gy.xz);
    let tn = unpack(t.detail);

    var s: Surface;
    s.albedo = t.albedo.rgb;
    s.height = t.albedo.a;
    s.normal = normalize(vec3<f32>(tn.x + n.x, tn.z * n.y, -tn.y + n.z));
    s.occlusion = t.detail.a;
    s.roughness = t.detail.b;
    return s;
}

fn triplanar(layer: i32, p: vec3<f32>, n: vec3<f32>, gx: vec3<f32>, gy: vec3<f32>) -> Surface {
    var blend = pow(abs(n), vec3<f32>(4.0));
    blend = blend / (blend.x + blend.y + blend.z);

    let x = fetch_tiled(layer, p.zy, gx.zy, gy.zy);
    let y = fetch_tiled(layer, p.xz, gx.xz, gy.xz);
    let z = fetch_tiled(layer, p.xy, gx.xy, gy.xy);

    let tx = unpack(x.detail);
    let ty = unpack(y.detail);
    let tz = unpack(z.detail);
    let wx = vec3<f32>(tx.xy + n.zy, tx.z * n.x).zyx;
    let wy = vec3<f32>(ty.xy + n.xz, ty.z * n.y).xzy;
    let wz = vec3<f32>(tz.xy + n.xy, tz.z * n.z);

    let albedo = x.albedo * blend.x + y.albedo * blend.y + z.albedo * blend.z;
    let detail = x.detail * blend.x + y.detail * blend.y + z.detail * blend.z;

    var s: Surface;
    s.albedo = albedo.rgb;
    s.height = albedo.a;
    s.normal = normalize(wx * blend.x + wy * blend.y + wz * blend.z);
    s.occlusion = detail.a;
    s.roughness = detail.b;
    return s;
}

fn project(layer: i32, tri: bool, world: vec3<f32>, n: vec3<f32>, dwx: vec3<f32>, dwy: vec3<f32>) -> Surface {
    let scale = palette.texture_scale;
    let p = world / scale;
    let gx = dwx / scale;
    let gy = dwy / scale;
    if tri {
        return triplanar(layer, p, n, gx, gy);
    }
    return planar(layer, p, n, gx, gy);
}

fn layer_surface(
    layer: i32,
    tri: bool,
    enabled: bool,
    world: vec3<f32>,
    n: vec3<f32>,
    dwx: vec3<f32>,
    dwy: vec3<f32>,
    view_distance: f32,
) -> Surface {
    let top = f32(textureNumLevels(albedo_tex) - 1u);
    let average = textureSampleLevel(albedo_tex, albedo_sampler, vec2<f32>(0.5), layer, top);
    let average_detail = textureSampleLevel(normal_tex, normal_sampler, vec2<f32>(0.5), layer, top);

    var s: Surface;
    s.albedo = average.rgb;
    s.height = average.a;
    s.normal = n;
    s.occlusion = average_detail.a;
    s.roughness = average_detail.b;

    if !enabled {
        return s;
    }

    let texture_weight = 1.0 - smoothstep(palette.far_distance * 0.5, palette.far_distance, view_distance);
    if texture_weight <= 0.001 {
        return s;
    }

    let textured = project(layer, tri, world, n, dwx, dwy);
    let normal_weight = 1.0 - smoothstep(palette.detail_distance * 0.3, palette.detail_distance, view_distance);

    s.albedo = mix(s.albedo, textured.albedo, texture_weight);
    s.height = mix(s.height, textured.height, texture_weight);
    s.occlusion = mix(s.occlusion, textured.occlusion, texture_weight);
    s.roughness = mix(s.roughness, textured.roughness, texture_weight);
    s.normal = normalize(mix(n, textured.normal, normal_weight * texture_weight));
    return s;
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

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    let world = input.world;
    let xz = world.xz;
    let n = normalize(input.normal);
    let dwx = dpdx(world);
    let dwy = dpdy(world);
    let view_distance = length(world - cam_uniform.position);

    let border = vec4<f32>(
        fbm(xz * 0.21),
        fbm(xz * 0.23 + vec2<f32>(41.0, 7.0)),
        fbm(xz * 0.19 + vec2<f32>(13.0, 59.0)),
        fbm(xz * 0.17 + vec2<f32>(91.0, 23.0)),
    );
    var weights = max(input.splat, vec4<f32>(1e-4)) * mix(vec4<f32>(1.0), border * 2.0, palette.border_noise);
    weights = pow(weights, vec4<f32>(palette.sharpness));
    weights = weights / max(dot(weights, vec4<f32>(1.0)), 1e-4);

    let used = weights > vec4<f32>(0.002);
    let grass = layer_surface(0, false, used.x, world, n, dwx, dwy, view_distance);
    let soil = layer_surface(1, false, used.y, world, n, dwx, dwy, view_distance);
    let rocky = layer_surface(2, true, used.z, world, n, dwx, dwy, view_distance);
    let cliff = layer_surface(3, true, used.w, world, n, dwx, dwy, view_distance);

    let heights = vec4<f32>(grass.height, soil.height, rocky.height, cliff.height);
    let raised = heights + weights;
    let highest = max(max(raised.x, raised.y), max(raised.z, raised.w)) - max(palette.height_blend, 1e-3);
    var blend = max(raised - vec4<f32>(highest), vec4<f32>(0.0));
    blend = select(vec4<f32>(0.0), blend, used);
    blend = blend / max(dot(blend, vec4<f32>(1.0)), 1e-4);

    var albedo = grass.albedo * palette.grass.rgb * blend.x
        + soil.albedo * palette.soil.rgb * blend.y
        + rocky.albedo * palette.rocky.rgb * blend.z
        + cliff.albedo * palette.cliff.rgb * blend.w;
    let normal = normalize(grass.normal * blend.x + soil.normal * blend.y + rocky.normal * blend.z + cliff.normal * blend.w);
    let occlusion = dot(vec4<f32>(grass.occlusion, soil.occlusion, rocky.occlusion, cliff.occlusion), blend);
    let roughness = clamp(dot(vec4<f32>(grass.roughness, soil.roughness, rocky.roughness, cliff.roughness), blend), 0.05, 1.0);

    let p = xz / max(palette.macro_wavelength, 1.0);
    let variation = value_noise(p) * 0.6 + value_noise(p * 3.1) * 0.4;
    albedo = albedo * (1.0 + (variation - 0.5) * 2.0 * palette.macro_strength);

    let moisture = input.hints.x * palette.moisture_strength;
    let sediment = input.hints.y * palette.moisture_strength;
    albedo = albedo * mix(vec3<f32>(1.0), vec3<f32>(0.78, 0.86, 0.74), moisture);
    albedo = mix(albedo, albedo * vec3<f32>(1.1, 1.0, 0.86), sediment);

    let sun_dir = normalize(light.sun.xyz);
    let sun_color = vec3<f32>(1.0, 0.95, 0.85) * light.sun.w;
    let n_dot_l = max(dot(normal, sun_dir), 0.0);
    let shadow = shadow_factor(world, n);

    let view_dir = normalize(cam_uniform.position - world);
    let half_dir = normalize(sun_dir + view_dir);
    let alpha = roughness * roughness;
    let shininess = clamp(2.0 / max(alpha * alpha, 1e-4) - 2.0, 1.0, 2048.0);
    let fresnel = 0.04 + 0.96 * pow(1.0 - max(dot(half_dir, view_dir), 0.0), 5.0);
    let specular = pow(max(dot(normal, half_dir), 0.0), shininess) * (shininess + 8.0) / 25.13 * fresnel * palette.specular;

    let ground = vec3<f32>(0.25, 0.22, 0.18);
    let ambient = mix(ground, palette.sky.rgb, normal.y * 0.5 + 0.5) * 0.4 * occlusion;
    let direct = sun_color * n_dot_l * shadow * mix(1.0, occlusion, 0.5);
    var color = albedo * (direct * 0.9 + ambient) + sun_color * specular * n_dot_l * shadow;

    let fog = 1.0 - exp(-view_distance * palette.fog_density);
    color = mix(color, palette.sky.rgb, clamp(fog, 0.0, 1.0));

    return vec4<f32>(color, 1.0);
}

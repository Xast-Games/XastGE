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
    far_scale: f32,
    stochastic: f32,
    padding: f32,
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

var<push_constant> transform: mat4x4<f32>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) splat: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) splat: vec4<f32>,
}

@vertex
fn vertex(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world = transform * vec4<f32>(input.position, 1.0);

    out.world = world.xyz;
    out.normal = (transform * vec4<f32>(input.normal, 0.0)).xyz;
    out.splat = input.splat;
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
    return t.xyz * 2.0 - 1.0;
}

fn planar(layer: i32, p: vec3<f32>, n: vec3<f32>, gx: vec3<f32>, gy: vec3<f32>) -> Surface {
    let t = fetch_tiled(layer, p.xz, gx.xz, gy.xz);
    let tn = unpack(t.detail);

    var s: Surface;
    s.albedo = t.albedo.rgb;
    s.height = t.albedo.a;
    s.normal = normalize(vec3<f32>(tn.x + n.x, abs(tn.z) * n.y, -tn.y + n.z));
    s.occlusion = t.detail.a;
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
    let wx = vec3<f32>(tx.xy + n.zy, abs(tx.z) * n.x).zyx;
    let wy = vec3<f32>(ty.xy + n.xz, abs(ty.z) * n.y).xzy;
    let wz = vec3<f32>(tz.xy + n.xy, abs(tz.z) * n.z);

    let albedo = x.albedo * blend.x + y.albedo * blend.y + z.albedo * blend.z;

    var s: Surface;
    s.albedo = albedo.rgb;
    s.height = albedo.a;
    s.normal = normalize(wx * blend.x + wy * blend.y + wz * blend.z);
    s.occlusion = x.detail.a * blend.x + y.detail.a * blend.y + z.detail.a * blend.z;
    return s;
}

fn project(layer: i32, tri: bool, scale: f32, world: vec3<f32>, n: vec3<f32>, dwx: vec3<f32>, dwy: vec3<f32>) -> Surface {
    let p = world / scale;
    let gx = dwx / scale;
    let gy = dwy / scale;
    if tri {
        return triplanar(layer, p, n, gx, gy);
    }
    return planar(layer, p, n, gx, gy);
}

fn mix_surface(a: Surface, b: Surface, t: f32) -> Surface {
    var s: Surface;
    s.albedo = mix(a.albedo, b.albedo, t);
    s.height = mix(a.height, b.height, t);
    s.normal = normalize(mix(a.normal, b.normal, t));
    s.occlusion = mix(a.occlusion, b.occlusion, t);
    return s;
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

    if !enabled {
        return s;
    }

    let far_weight = 1.0 - smoothstep(palette.far_distance * 0.4, palette.far_distance, view_distance);
    if far_weight > 0.001 {
        let far_surface = project(layer, tri, palette.texture_scale * max(palette.far_scale, 1.0), world, n, dwx, dwy);
        s = mix_surface(s, far_surface, far_weight);
    }

    let near_weight = 1.0 - smoothstep(palette.detail_distance * 0.3, palette.detail_distance, view_distance);
    if near_weight > 0.001 {
        let near_surface = project(layer, tri, palette.texture_scale, world, n, dwx, dwy);
        s = mix_surface(s, near_surface, near_weight);
    }

    return s;
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
    let top = max(max(raised.x, raised.y), max(raised.z, raised.w)) - max(palette.height_blend, 1e-3);
    var blend = max(raised - vec4<f32>(top), vec4<f32>(0.0));
    blend = select(vec4<f32>(0.0), blend, used);
    blend = blend / max(dot(blend, vec4<f32>(1.0)), 1e-4);

    var albedo = grass.albedo * palette.grass.rgb * blend.x
        + soil.albedo * palette.soil.rgb * blend.y
        + rocky.albedo * palette.rocky.rgb * blend.z
        + cliff.albedo * palette.cliff.rgb * blend.w;
    let normal = normalize(grass.normal * blend.x + soil.normal * blend.y + rocky.normal * blend.z + cliff.normal * blend.w);
    let occlusion = dot(vec4<f32>(grass.occlusion, soil.occlusion, rocky.occlusion, cliff.occlusion), blend);

    let p = xz / max(palette.macro_wavelength, 1.0);
    let variation = value_noise(p) * 0.6 + value_noise(p * 3.1) * 0.4;
    albedo = albedo * (1.0 + (variation - 0.5) * 2.0 * palette.macro_strength);

    let sun_dir = normalize(vec3<f32>(0.45, 0.8, 0.3));
    let sun = vec3<f32>(1.0, 0.95, 0.85) * max(dot(normal, sun_dir), 0.0);
    let ground = vec3<f32>(0.25, 0.22, 0.18);
    let ambient = mix(ground, palette.sky.rgb, normal.y * 0.5 + 0.5) * 0.45 * occlusion;
    var color = albedo * (sun * mix(1.0, occlusion, 0.5) * 0.9 + ambient);

    let fog = 1.0 - exp(-view_distance * palette.fog_density);
    color = mix(color, palette.sky.rgb, clamp(fog, 0.0, 1.0));

    return vec4<f32>(color, 1.0);
}

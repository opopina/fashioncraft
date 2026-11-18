// Graphics (prints, logos) lying on the cloth: an image sampled across clipped cloth
// triangles, lit like the fabric's diffuse response and blended over it by its alpha. The
// print follows the fabric under it: its weave, the relief of its texture and its normal map
// tilt the print's normal as they tilt the cloth's.
struct View {
    projection: mat4x4f,
    // xyz is the eye for perspective (w=1), or direction to eye for ortho (w=0).
    eye: vec4f,
    light: vec4f, // world direction, key gain
    ambient: vec4f, // ambient gain, padding
};
@group(0) @binding(0) var<uniform> view: View;
@group(1) @binding(0) var image: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;
@group(2) @binding(0) var fabric_maps: texture_2d_array<f32>;
@group(2) @binding(1) var fabric_sampler: sampler;

struct VertexOut {
    @builtin(position) position: vec4f,
    @location(0) normal: vec3f,
    @location(1) world: vec3f,
    @location(2) uv: vec2f,
    @location(3) opacity: f32,
    // The fabric under the print: weave coordinates (warp, weft); weave kind, thread spacing
    // and depth; texture coordinates; texture layer, relief height in metres, normal-map
    // layer and roughness-map layer (-1 for none).
    @location(4) threads_uv: vec2f,
    @location(5) weave: vec3f,
    @location(6) map_uv: vec2f,
    @location(7) map: vec4f,
};

@vertex
fn vs_main(@location(0) world: vec3f, @location(1) normal: vec3f, @location(2) uv: vec2f,
           @location(3) opacity: f32, @location(4) threads_uv: vec2f, @location(5) weave: vec3f,
           @location(6) map_uv: vec2f, @location(7) map: vec4f) -> VertexOut {
    var out: VertexOut;
    out.opacity = opacity;
    out.position = view.projection * vec4f(world, 1.0);
    out.normal = normal;
    out.world = world;
    out.uv = uv;
    out.threads_uv = threads_uv;
    out.weave = weave;
    out.map_uv = map_uv;
    out.map = map;
    return out;
}

fn encode_srgb(linear: vec3f) -> vec3f {
    let c = clamp(linear, vec3f(0.0), vec3f(1.0));
    return select(1.055 * pow(c, vec3f(1.0 / 2.4)) - 0.055, 12.92 * c, c <= vec3f(0.0031308));
}

// The same procedural weave as the cloth's: the top thread's height (0..1) and its slope at
// thread coordinates. 1 plain, 2 twill, 3 satin, 4 knit.
fn weave_height(c: vec2f, kind: f32) -> vec3f {
    let cell = floor(c);
    let f = fract(c) - vec2f(0.5);
    let i = i32(cell.x);
    let j = i32(cell.y);
    var warp_up = false;
    if kind < 1.5 {
        warp_up = ((i + j) & 1) == 0;
    } else if kind < 2.5 {
        warp_up = (((i + j) % 3) + 3) % 3 != 0;
    } else if kind < 3.5 {
        warp_up = (((2 * j - i) % 5) + 5) % 5 != 0;
    } else {
        let v = abs(f.x) * 2.0;
        let loop_t = fract(c.y + 0.5 * v) - 0.5;
        let h = cos(3.14159 * loop_t) * cos(1.5708 * v);
        return vec3f(h, -1.5708 * sign(f.x) * h, -3.14159 * sin(3.14159 * loop_t));
    }
    if warp_up {
        let h = cos(3.14159 * f.x) * (0.75 + 0.25 * cos(6.28318 * f.y));
        return vec3f(h, -3.14159 * sin(3.14159 * f.x), -1.5708 * sin(6.28318 * f.y));
    }
    let h = cos(3.14159 * f.y) * (0.75 + 0.25 * cos(6.28318 * f.x));
    return vec3f(h, -1.5708 * sin(6.28318 * f.x), -3.14159 * sin(3.14159 * f.y));
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4f {
    let texel = textureSample(image, image_sampler, in.uv);
    let to_eye = view.eye.xyz - in.world * view.eye.w;
    var n = in.normal / max(length(in.normal), 0.000001);
    // Seen from either side the print is lit on the side facing the viewer.
    if dot(n, to_eye) < 0.0 {
        n = -n;
    }
    // Derivatives and fabric samples in uniform control flow, before any discard.
    let threads = in.threads_uv / max(in.weave.y, 0.00001);
    let thread_px = max(length(fwidth(threads)), 0.00001);
    let dp_dx = dpdx(in.world);
    let dp_dy = dpdy(in.world);
    let dc_dx = dpdx(threads);
    let dc_dy = dpdy(threads);
    let dmap_dx = dpdx(in.map_uv);
    let dmap_dy = dpdy(in.map_uv);
    let layer = i32(max(in.map.x, 0.0));
    let step = 1.0 / vec2f(textureDimensions(fabric_maps));
    let fabric = textureSample(fabric_maps, fabric_sampler, in.map_uv, layer).rgb;
    let fabric_u = textureSample(fabric_maps, fabric_sampler, in.map_uv + vec2f(step.x, 0.0), layer).rgb;
    let fabric_v = textureSample(fabric_maps, fabric_sampler, in.map_uv + vec2f(0.0, step.y), layer).rgb;
    let normal_texel = textureSample(fabric_maps, fabric_sampler, in.map_uv, i32(max(in.map.z, 0.0))).rgb;
    let alpha = texel.a * in.opacity;
    if alpha < 0.004 {
        discard;
    }
    // The weave's relief, in the surface's own frame.
    if in.weave.x > 0.5 {
        let w = weave_height(threads, in.weave.x);
        let fade = clamp(1.5 - thread_px * 1.5, 0.0, 1.0);
        let p2 = cross(dp_dy, n);
        let p1 = cross(n, dp_dx);
        let t = p2 * dc_dx.x + p1 * dc_dy.x;
        let b = p2 * dc_dx.y + p1 * dc_dy.y;
        let scale = inverseSqrt(max(max(dot(t, t), dot(b, b)), 1e-20));
        n = normalize(n - 0.35 * in.weave.z * fade * (w.y * t + w.z * b) * scale);
    }
    // The fabric texture's brightness as height, then its normal map.
    if in.map.x > -0.5 {
        let luma = vec3f(0.2126, 0.7152, 0.0722);
        let slope = vec2f(dot(fabric_u - fabric, luma), dot(fabric_v - fabric, luma)) / step;
        let r1 = cross(dp_dy, n);
        let r2 = cross(n, dp_dx);
        let det = dot(dp_dx, r1);
        if abs(det) > 1e-24 {
            let grad_u = (r1 * dmap_dx.x + r2 * dmap_dy.x) / det;
            let grad_v = (r1 * dmap_dx.y + r2 * dmap_dy.y) / det;
            n = normalize(n - in.map.y * (slope.x * grad_u + slope.y * grad_v));
            if in.map.z > -0.5 {
                let t = grad_u / max(length(grad_u), 1e-12);
                let b0 = -grad_v / max(length(grad_v), 1e-12);
                let b = normalize(b0 - n * dot(b0, n));
                let m = normal_texel * 2.0 - vec3f(1.0);
                n = normalize(t * m.x + b * m.y + n * max(m.z, 0.05));
            }
        }
    }
    let nl = clamp(dot(n, normalize(view.light.xyz)), 0.0, 1.0);
    let rgb = texel.rgb * (0.25 * view.ambient.x + 0.7 * view.light.w * nl);
    return vec4f(encode_srgb(rgb), alpha);
}

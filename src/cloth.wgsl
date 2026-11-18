struct View {
    projection: mat4x4f,
    // xyz is the eye for perspective (w=1), or direction to eye for ortho (w=0).
    eye: vec4f,
    light: vec4f, // world direction, key gain
    ambient: vec4f, // ambient gain, padding
};
@group(0) @binding(0) var<uniform> view: View;
// Fabric textures, one per layer; a vertex's map.z picks its layer (-1 for none).
@group(1) @binding(0) var fabric_maps: texture_2d_array<f32>;
@group(1) @binding(1) var fabric_sampler: sampler;
struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) barycentric: vec3<f32>,
    @location(3) world: vec3f,
    @location(4) roughness: f32,
    @location(5) back: vec3f,
    @location(6) uv: vec2f,
    @location(7) weave: vec3f,
    // Texture coordinates (one repeat spans [0,1]), layer, relief height in metres.
    @location(8) map: vec4f,
};

@vertex
fn vs_main(@location(0) clip: vec4<f32>, @location(1) normal: vec3<f32>,
           @location(2) color: vec4<f32>, @location(3) bary: vec3<f32>, @location(4) roughness: f32,
           @location(5) back: vec3f, @location(6) uv: vec2f, @location(7) weave: vec3f,
           @location(8) map: vec4f) -> VertexOut {
    var out: VertexOut;
    out.map = map;
    out.back = back;
    out.uv = uv;
    out.weave = weave;
    out.position = view.projection * clip;
    out.world = clip.xyz;
    out.roughness = roughness;
    out.normal = normal;
    out.color = color;
    out.barycentric = bary;
    return out;
}

// Explicit custom optics use a bounded dielectric GGX highlight. This is a
// presentation model, not a measured textile BRDF. Legacy/diagnostic colors
// retain their existing lighting and encoded-output path.
fn encode_srgb(linear: vec3f) -> vec3f {
    let c = clamp(linear, vec3f(0.0), vec3f(1.0));
    return select(1.055 * pow(c, vec3f(1.0 / 2.4)) - 0.055, 12.92 * c, c <= vec3f(0.0031308));
}

// Procedural weave at thread coordinates (warp along x, weft along y): the height of the top
// thread (0..1) and its slope, for shading and relief. 1 plain, 2 twill, 3 satin, 4 knit.
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
        // Knit: columns of V-shaped loops.
        let v = abs(f.x) * 2.0;
        let loop_t = fract(c.y + 0.5 * v) - 0.5;
        let h = cos(3.14159 * loop_t) * cos(1.5708 * v);
        return vec3f(h, -1.5708 * sign(f.x) * h, -3.14159 * sin(3.14159 * loop_t));
    }
    // The top thread bulges across its width and dips where it passes under the next one.
    if warp_up {
        let h = cos(3.14159 * f.x) * (0.75 + 0.25 * cos(6.28318 * f.y));
        return vec3f(h, -3.14159 * sin(3.14159 * f.x), -1.5708 * sin(6.28318 * f.y));
    }
    let h = cos(3.14159 * f.y) * (0.75 + 0.25 * cos(6.28318 * f.x));
    return vec3f(h, -1.5708 * sin(6.28318 * f.x), -3.14159 * sin(3.14159 * f.y));
}

@fragment
fn fs_main(in: VertexOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    var n = in.normal / max(length(in.normal), 0.000001) * select(-1.0, 1.0, front);
    let derivative = max(fwidth(in.barycentric), vec3<f32>(0.00001));
    let edges = smoothstep(vec3<f32>(0.0), derivative * 1.2, in.barycentric);
    let wire = min(edges.x, min(edges.y, edges.z));
    let wire_factor = mix(1.0, 0.28 + 0.72 * wire, in.color.a);
    // Derivatives in uniform control flow, for the weave's relief and its fade at a distance.
    let threads = in.uv / max(in.weave.y, 0.00001);
    let thread_px = max(length(fwidth(threads)), 0.00001);
    let dp_dx = dpdx(in.world);
    let dp_dy = dpdy(in.world);
    let dc_dx = dpdx(threads);
    let dc_dy = dpdy(threads);
    // The fabric's front is the side its pattern shows on the 2D board, which faces away from
    // the faces' winding normal: a placed garment shows it outside.
    var base = select(in.color.rgb, in.back, front);
    // The fabric texture and two neighbouring texels for its relief, sampled in uniform
    // control flow; faces without a texture ignore them.
    let layer = i32(max(in.map.z, 0.0));
    let texel = 1.0 / vec2f(textureDimensions(fabric_maps));
    let image = textureSample(fabric_maps, fabric_sampler, in.map.xy, layer).rgb;
    let image_u = textureSample(fabric_maps, fabric_sampler, in.map.xy + vec2f(texel.x, 0.0), layer).rgb;
    let image_v = textureSample(fabric_maps, fabric_sampler, in.map.xy + vec2f(0.0, texel.y), layer).rgb;
    let dmap_dx = dpdx(in.map.xy);
    let dmap_dy = dpdy(in.map.xy);
    if in.weave.x > 0.5 {
        let w = weave_height(threads, in.weave.x);
        // Threads finer than about a pixel fade to their mean, instead of shimmering.
        let fade = clamp(1.5 - thread_px * 1.5, 0.0, 1.0);
        base = base * mix(1.0 - 0.5 * in.weave.z, 1.0 - in.weave.z * (1.0 - w.x), fade);
        // Relief: tilt the normal along the thread slope, in the surface's own frame.
        let p2 = cross(dp_dy, n);
        let p1 = cross(n, dp_dx);
        let t = p2 * dc_dx.x + p1 * dc_dy.x;
        let b = p2 * dc_dx.y + p1 * dc_dy.y;
        let scale = inverseSqrt(max(max(dot(t, t), dot(b, b)), 1e-20));
        n = normalize(n - 0.35 * in.weave.z * fade * (w.y * t + w.z * b) * scale);
    }
    if in.map.z > -0.5 {
        base = base * image;
        // Brightness as height: tilt the normal against its gradient along the surface.
        let luma = vec3f(0.2126, 0.7152, 0.0722);
        let slope = vec2f(dot(image_u - image, luma), dot(image_v - image, luma)) / texel;
        let r1 = cross(dp_dy, n);
        let r2 = cross(n, dp_dx);
        let det = dot(dp_dx, r1);
        if abs(det) > 1e-24 {
            let grad_u = (r1 * dmap_dx.x + r2 * dmap_dy.x) / det;
            let grad_v = (r1 * dmap_dx.y + r2 * dmap_dy.y) / det;
            n = normalize(n - in.map.w * (slope.x * grad_u + slope.y * grad_v));
        }
    }
    let lighting = 0.35 * view.ambient.x + 0.65 * view.light.w * abs(dot(n, normalize(view.light.xyz)));
    if in.roughness < 0.0 {
        return vec4f(base * lighting * wire_factor, 1.0);
    }
    let l = normalize(view.light.xyz);
    let to_eye = view.eye.xyz - in.world * view.eye.w;
    let v = to_eye / max(length(to_eye), 0.000001);
    let h = (l + v) / max(length(l + v), 0.000001);
    let nl = clamp(dot(n, l), 0.0, 1.0);
    let nv = clamp(dot(n, v), 0.0001, 1.0);
    let nh = clamp(dot(n, h), 0.0, 1.0);
    let vh = clamp(dot(v, h), 0.0, 1.0);
    let alpha = max(in.roughness * in.roughness, 0.02);
    let a2 = alpha * alpha;
    let denom = nh * nh * (a2 - 1.0) + 1.0;
    let distribution = a2 / (3.14159265 * denom * denom);
    let gv = 2.0 * nv / (nv + sqrt(a2 + (1.0 - a2) * nv * nv));
    let gl = 2.0 * nl / max(nl + sqrt(a2 + (1.0 - a2) * nl * nl), 0.0001);
    let fresnel = 0.04 + 0.96 * pow(1.0 - vh, 5.0);
    let specular = distribution * gv * gl * fresnel / max(4.0 * nv, 0.0001);
    let rgb = (base * (0.25 * view.ambient.x + 0.7 * view.light.w * nl * (1.0 - fresnel)) + vec3f(0.2 * view.light.w * specular)) * wire_factor;
    return vec4f(encode_srgb(rgb), 1.0);
}

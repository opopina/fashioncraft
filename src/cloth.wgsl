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
    // Layers of the normal map and the roughness map (-1 for none).
    @location(9) detail: vec2f,
    // Layers of the metalness, opacity, displacement and ambient-occlusion maps (-1 for none).
    @location(10) extra: vec4f,
    // Metalness (without a map) and displacement height in metres.
    @location(11) params: vec4f,
};

@vertex
fn vs_main(@location(0) clip: vec4<f32>, @location(1) normal: vec3<f32>,
           @location(2) color: vec4<f32>, @location(3) bary: vec3<f32>, @location(4) roughness: f32,
           @location(5) back: vec3f, @location(6) uv: vec2f, @location(7) weave: vec3f,
           @location(8) map: vec4f, @location(9) detail: vec2f,
           @location(10) extra: vec4f, @location(11) params: vec4f) -> VertexOut {
    var out: VertexOut;
    out.extra = extra;
    out.params = params;
    out.map = map;
    out.detail = detail;
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
    return shade(in, front);
}

/// The garment drawn see-through (Display › Garment style › Translucent), blended over the
/// avatar and the rest of the scene.
@fragment
fn fs_translucent(in: VertexOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    return vec4f(shade(in, front).rgb, TRANSLUCENT_ALPHA);
}

const TRANSLUCENT_ALPHA: f32 = 0.45;

fn shade(in: VertexOut, front: bool) -> vec4<f32> {
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
    let has_map = in.map.z > -0.5;
    let dmap_dx = dpdx(in.map.xy);
    let dmap_dy = dpdy(in.map.xy);
    // Gradients of the map coordinates along the surface (per metre), for relief, normal maps
    // and parallax.
    let r1 = cross(dp_dy, n);
    let r2 = cross(n, dp_dx);
    let det = dot(dp_dx, r1);
    let frame_ok = abs(det) > 1e-24;
    let grad_u = (r1 * dmap_dx.x + r2 * dmap_dy.x) / select(1.0, det, frame_ok);
    let grad_v = (r1 * dmap_dx.y + r2 * dmap_dy.y) / select(1.0, det, frame_ok);
    let to_eye = view.eye.xyz - in.world * view.eye.w;
    let v = to_eye / max(length(to_eye), 0.000001);
    // Displacement drawn by parallax: the surface sits at the map's white, and the view ray
    // walks down into the height field to where it meets it. Every sample past this point
    // reads at the shifted coordinates with the original footprint.
    var uv = in.map.xy;
    let height_layer = i32(max(in.extra.z, 0.0));
    let displaced = has_map && in.extra.z > -0.5 && in.params.y > 0.0 && frame_ok;
    if displaced {
        let nv = max(dot(n, v), 0.2);
        let shift = -vec2f(dot(grad_u, v), dot(grad_v, v)) / nv * in.params.y;
        var previous_uv = uv;
        var previous_gap = 1.0;
        for (var k = 0; k <= 24; k++) {
            let depth = f32(k) / 24.0;
            let at = in.map.xy + shift * depth;
            let h = textureSampleGrad(fabric_maps, fabric_sampler, at, height_layer, dmap_dx, dmap_dy).r;
            let gap = (1.0 - h) - depth;
            if gap <= 0.0 {
                let w = select(0.0, previous_gap / max(previous_gap - gap, 1e-6), k > 0);
                uv = mix(previous_uv, at, w);
                break;
            }
            previous_uv = at;
            previous_gap = gap;
            uv = at;
        }
    }
    // The fabric texture and two neighbouring texels for its relief; faces without a texture
    // ignore them.
    let layer = i32(max(in.map.z, 0.0));
    let texel = 1.0 / vec2f(textureDimensions(fabric_maps));
    let image_rgba = textureSampleGrad(fabric_maps, fabric_sampler, uv, layer, dmap_dx, dmap_dy);
    let image = image_rgba.rgb;
    let image_u = textureSampleGrad(fabric_maps, fabric_sampler, uv + vec2f(texel.x, 0.0), layer, dmap_dx, dmap_dy).rgb;
    let image_v = textureSampleGrad(fabric_maps, fabric_sampler, uv + vec2f(0.0, texel.y), layer, dmap_dx, dmap_dy).rgb;
    // The maps, stored so that sampling returns their values (a DirectX normal map's green
    // already flipped, a glossiness map already inverted).
    let normal_texel = textureSampleGrad(fabric_maps, fabric_sampler, uv, i32(max(in.detail.x, 0.0)), dmap_dx, dmap_dy).rgb;
    let rough_texel = textureSampleGrad(fabric_maps, fabric_sampler, uv, i32(max(in.detail.y, 0.0)), dmap_dx, dmap_dy).r;
    let metal_texel = textureSampleGrad(fabric_maps, fabric_sampler, uv, i32(max(in.extra.x, 0.0)), dmap_dx, dmap_dy).r;
    let opacity_texel = textureSampleGrad(fabric_maps, fabric_sampler, uv, i32(max(in.extra.y, 0.0)), dmap_dx, dmap_dy);
    let occlusion_texel = textureSampleGrad(fabric_maps, fabric_sampler, uv, i32(max(in.extra.w, 0.0)), dmap_dx, dmap_dy).r;
    // A negative relief marks a cut-out image (hair, eyelashes): clear texels are not drawn.
    if has_map && in.map.w < 0.0 && image_rgba.a < 0.4 {
        discard;
    }
    // Lace and mesh: the opacity map cuts the cloth away where it is under one half.
    if has_map && in.extra.y > -0.5 && opacity_texel.r * opacity_texel.a < 0.5 {
        discard;
    }
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
    if has_map {
        base = base * image;
        // Brightness as height: tilt the normal against its gradient along the surface.
        let luma = vec3f(0.2126, 0.7152, 0.0722);
        let slope = vec2f(dot(image_u - image, luma), dot(image_v - image, luma)) / texel;
        if frame_ok {
            n = normalize(n - max(in.map.w, 0.0) * (slope.x * grad_u + slope.y * grad_v));
            // A normal map in the repeat's frame: red along it, green up it (against the
            // image's rows), blue out of the cloth.
            if in.detail.x > -0.5 {
                let t = grad_u / max(length(grad_u), 1e-12);
                let b0 = -grad_v / max(length(grad_v), 1e-12);
                let b = normalize(b0 - n * dot(b0, n));
                let m = normal_texel * 2.0 - vec3f(1.0);
                n = normalize(t * m.x + b * m.y + n * max(m.z, 0.05));
            } else if displaced {
                // Without a normal map, the displacement shades its own relief.
                let h = textureSampleGrad(fabric_maps, fabric_sampler, uv, height_layer, dmap_dx, dmap_dy).r;
                let h_u = textureSampleGrad(fabric_maps, fabric_sampler, uv + vec2f(texel.x, 0.0), height_layer, dmap_dx, dmap_dy).r;
                let h_v = textureSampleGrad(fabric_maps, fabric_sampler, uv + vec2f(0.0, texel.y), height_layer, dmap_dx, dmap_dy).r;
                let rise = vec2f(h_u - h, h_v - h) / texel;
                n = normalize(n - in.params.y * (rise.x * grad_u + rise.y * grad_v));
            }
        }
    }
    let lighting = 0.35 * view.ambient.x + 0.65 * view.light.w * abs(dot(n, normalize(view.light.xyz)));
    if in.roughness < 0.0 {
        return vec4f(base * lighting * wire_factor, 1.0);
    }
    let l = normalize(view.light.xyz);
    let h = (l + v) / max(length(l + v), 0.000001);
    let nl = clamp(dot(n, l), 0.0, 1.0);
    let nv = clamp(dot(n, v), 0.0001, 1.0);
    let nh = clamp(dot(n, h), 0.0, 1.0);
    let vh = clamp(dot(v, h), 0.0, 1.0);
    let roughness = select(in.roughness, rough_texel, has_map && in.detail.y > -0.5);
    let metal = clamp(select(in.params.x, metal_texel, has_map && in.extra.x > -0.5), 0.0, 1.0);
    let occlusion = select(1.0, occlusion_texel, has_map && in.extra.w > -0.5);
    let alpha = max(roughness * roughness, 0.02);
    let a2 = alpha * alpha;
    let denom = nh * nh * (a2 - 1.0) + 1.0;
    let distribution = a2 / (3.14159265 * denom * denom);
    let gv = 2.0 * nv / (nv + sqrt(a2 + (1.0 - a2) * nv * nv));
    let gl = 2.0 * nl / max(nl + sqrt(a2 + (1.0 - a2) * nl * nl), 0.0001);
    // Metals reflect in their own colour and have no diffuse part.
    let f0 = mix(vec3f(0.04), base, metal);
    let fresnel = f0 + (vec3f(1.0) - f0) * pow(1.0 - vh, 5.0);
    let specular = distribution * gv * gl * fresnel / max(4.0 * nv, 0.0001);
    let diffuse = base * (1.0 - metal);
    // A metal shows the room around it: a dim, rough stand-in for the environment.
    let surroundings = f0 * metal * 0.45 * view.ambient.x * occlusion;
    let rgb = (diffuse * (0.25 * view.ambient.x * occlusion + 0.7 * view.light.w * nl * (vec3f(1.0) - fresnel))
        + view.light.w * mix(0.2, 1.0, metal) * specular + surroundings) * wire_factor;
    return vec4f(encode_srgb(rgb), 1.0);
}

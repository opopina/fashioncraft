// Progressive path tracer for the photoreal render. One sample per pixel per dispatch is added
// to a running sum; `resolve` turns the mean into display pixels.
//
// Surfaces are the viewport's: the same per-face materials (front and back colours, weave,
// fabric texture and its maps) on the same triangles, walked through a bounding-volume
// hierarchy built on the CPU. Light comes from the key light (a small sun), the environment
// (an HDR image, importance sampled and combined with the surface's own sampling by multiple
// importance sampling, or the studio's sky colours) and bounces between surfaces and the floor.
// The BSDF is the viewport's: a GGX microfacet lobe with Schlick's Fresnel over a Lambert lobe,
// metals reflecting in their own colour. It is a presentation model, not a measured textile
// BRDF: no sheen, no light through the cloth.

struct Frame {
    eye: vec4f,        // camera position; w = 1 for orthographic
    forward: vec4f,    // view direction; w = tan(half vertical field of view)
    right: vec4f,      // w = aspect (width / height)
    up: vec4f,         // w = orthographic half height (m)
    lens: vec4f,       // aperture radius (m), focus distance (m), exposure gain, max bounces
    sun: vec4f,        // direction towards the sun; w = cosine of its angular radius
    sun_color: vec4f,  // irradiance it brings to a surface facing it (linear rgb)
    sky_top: vec4f,    // studio sky above (linear rgb); w = environment gain
    sky_bottom: vec4f, // studio sky below (linear rgb); w = environment turn (radians)
    ground: vec4f,     // floor colour (linear rgb); w = floor height (m)
    options: vec4f,    // floor shown, HDR environment, environment as background, spare
    backdrop_top: vec4f,    // light behind the scene at the image's top, as the viewport shows it
    backdrop_bottom: vec4f, // and at its bottom
    size: vec4u,       // image width, height, environment width, height
    pass_info: vec4u,  // sample index, first row of this dispatch, rows in it, seed
};

struct Node {
    min: vec3f,
    start: u32,
    max: vec3f,
    count: u32,
};

struct Material {
    front: vec4f,  // colour, roughness (negative: the viewport's plain unlit colour)
    back: vec4f,   // back-face colour
    weave: vec4f,  // kind (0 none), thread spacing (m), contrast
    maps: vec4f,   // colour layer (-1 none), relief (m; negative: cut-out image), normal, roughness
    extra: vec4f,  // metalness, opacity, displacement and occlusion layers (-1 none)
    params: vec4f, // metalness, displacement height (m), opacity, kind (1 a print)
};

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<storage, read> nodes: array<Node>;
// Nine vectors a triangle: its corners (the first one's w holds the material), then for each
// corner its normal and warp coordinate, and its weft coordinate and texture coordinates.
@group(0) @binding(2) var<storage, read> triangles: array<vec4f>;
@group(0) @binding(3) var<storage, read> materials: array<Material>;
@group(0) @binding(4) var maps: texture_2d_array<f32>;
@group(0) @binding(5) var map_sampler: sampler;
// The environment's linear colours, row by row (top row first).
@group(0) @binding(6) var<storage, read> environment: array<vec4f>;
// Its sampling tables: each row's cumulative distribution (width + 1 values per row), then the
// rows' cumulative distribution (height + 1 values).
@group(0) @binding(7) var<storage, read> environment_cdf: array<f32>;
@group(0) @binding(8) var<storage, read_write> accumulation: array<vec4f>;
@group(0) @binding(9) var<storage, read_write> display: array<u32>;
// Samples that came out not finite, and were dropped; a converged render has none.
@group(0) @binding(10) var<storage, read_write> rejected: array<atomic<u32>>;

const PI: f32 = 3.14159265;
const NONE: u32 = 0xffffffffu;
const FLOOR: u32 = 0xfffffffeu;
const EPSILON: f32 = 0.0002;

var<private> rng: u32;

fn pcg(v: u32) -> u32 {
    let state = v * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn random() -> f32 {
    rng = pcg(rng);
    return f32(rng >> 8u) / 16777216.0;
}

// Whether every component is a finite number, by its bits: fast-math shader compilers may
// fold `x != x` away.
fn finite(v: vec3f) -> bool {
    let exponent = bitcast<vec3u>(v) & vec3u(0x7f800000u);
    return all(exponent != vec3u(0x7f800000u));
}

fn luminance(c: vec3f) -> f32 {
    return dot(c, vec3f(0.2126, 0.7152, 0.0722));
}

// An orthonormal frame around a unit normal (Duff et al. 2017).
fn basis(n: vec3f) -> mat3x3f {
    let s = select(-1.0, 1.0, n.z >= 0.0);
    let a = -1.0 / (s + n.z);
    let b = n.x * n.y * a;
    let t = vec3f(1.0 + s * n.x * n.x * a, s * b, -s * n.x);
    let u = vec3f(b, s + n.y * n.y * a, -n.y);
    return mat3x3f(t, u, n);
}

struct Hit {
    t: f32,
    id: u32,
    u: f32,
    v: f32,
};

fn slab(node: Node, origin: vec3f, inverse: vec3f, t_max: f32) -> bool {
    let t0 = (node.min - origin) * inverse;
    let t1 = (node.max - origin) * inverse;
    let near = max(max(min(t0.x, t1.x), min(t0.y, t1.y)), max(min(t0.z, t1.z), 0.0));
    let far = min(min(max(t0.x, t1.x), max(t0.y, t1.y)), min(max(t0.z, t1.z), t_max));
    return near <= far;
}

// The nearest surface along a ray: a triangle (by its index) or the floor.
fn nearest(origin: vec3f, direction: vec3f, t_max: f32) -> Hit {
    var hit = Hit(t_max, NONE, 0.0, 0.0);
    if frame.options.x > 0.5 && direction.y < 0.0 && origin.y > frame.ground.w {
        let t = (frame.ground.w - origin.y) / direction.y;
        if t < hit.t {
            hit = Hit(t, FLOOR, 0.0, 0.0);
        }
    }
    if arrayLength(&nodes) == 0u {
        return hit;
    }
    let safe = select(direction, vec3f(1e-12), abs(direction) < vec3f(1e-12));
    let inverse = 1.0 / safe;
    var stack: array<u32, 64>;
    var top = 1u;
    stack[0] = 0u;
    while top > 0u {
        top -= 1u;
        let index = stack[top];
        let node = nodes[index];
        if !slab(node, origin, inverse, hit.t) {
            continue;
        }
        if node.count > 0u {
            for (var k = node.start; k < node.start + node.count; k++) {
                let p0 = triangles[9u * k].xyz;
                let e1 = triangles[9u * k + 1u].xyz - p0;
                let e2 = triangles[9u * k + 2u].xyz - p0;
                let p = cross(direction, e2);
                let det = dot(e1, p);
                if abs(det) < 1e-14 {
                    continue;
                }
                let inv = 1.0 / det;
                let s = origin - p0;
                let u = dot(s, p) * inv;
                if u < 0.0 || u > 1.0 {
                    continue;
                }
                let q = cross(s, e1);
                let v = dot(direction, q) * inv;
                if v < 0.0 || u + v > 1.0 {
                    continue;
                }
                let t = dot(e2, q) * inv;
                if t > EPSILON && t < hit.t {
                    hit = Hit(t, k, u, v);
                }
            }
        } else if top + 2u <= 64u {
            stack[top] = node.start;
            stack[top + 1u] = index + 1u;
            top += 2u;
        }
    }
    return hit;
}

// Procedural weave at thread coordinates (warp along x, weft along y), as the viewport draws
// it: the top thread's height and its slope.
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

fn sample_map(uv: vec2f, layer: f32) -> vec4f {
    return textureSampleLevel(maps, map_sampler, uv, i32(max(layer, 0.0)), 0.0);
}

// What a ray sees where it hits.
struct Surface {
    position: vec3f,
    // Geometric and shading normals, both on the side the ray came from.
    geometric: vec3f,
    normal: vec3f,
    color: vec3f,
    roughness: f32,
    metal: f32,
    // Chance the ray stops here rather than passing through (lace, a print's clear parts).
    opacity: f32,
};

// The gradient along the triangle of a value that changes by d1 and d2 along its edges.
fn gradient(d1: f32, d2: f32, e1: vec3f, e2: vec3f, n: vec3f) -> vec3f {
    return (d1 * cross(e2, n) + d2 * cross(n, e1)) / max(dot(n, n), 1e-30);
}

fn surface_at(hit: Hit, origin: vec3f, direction: vec3f) -> Surface {
    var s: Surface;
    s.position = origin + direction * hit.t;
    s.opacity = 1.0;
    s.metal = 0.0;
    if hit.id == FLOOR {
        s.geometric = vec3f(0.0, 1.0, 0.0);
        s.normal = s.geometric;
        s.color = frame.ground.rgb;
        s.roughness = 0.85;
        return s;
    }
    let k = 9u * hit.id;
    let p0 = triangles[k];
    let e1 = triangles[k + 1u].xyz - p0.xyz;
    let e2 = triangles[k + 2u].xyz - p0.xyz;
    let m = materials[bitcast<u32>(p0.w)];
    let w = 1.0 - hit.u - hit.v;
    let a0 = triangles[k + 3u];
    let b0 = triangles[k + 4u];
    let a1 = triangles[k + 5u];
    let b1 = triangles[k + 6u];
    let a2 = triangles[k + 7u];
    let b2 = triangles[k + 8u];
    let ng = cross(e1, e2);
    // The viewport's front-facing side shows the back colour: the fabric's front is the side
    // facing away from the winding normal.
    let facing = dot(direction, ng) < 0.0;
    let side = select(-1.0, 1.0, facing);
    s.geometric = normalize(ng) * side;
    var n = normalize(a0.xyz * w + a1.xyz * hit.u + a2.xyz * hit.v) * side;
    if !finite(n) || dot(n, n) < 0.5 {
        n = s.geometric;
    }
    var color = select(m.front.rgb, m.back.rgb, facing);
    let uv = vec2f(a0.w, b0.x) * w + vec2f(a1.w, b1.x) * hit.u + vec2f(a2.w, b2.x) * hit.v;
    let map_uv = b0.yz * w + b1.yz * hit.u + b2.yz * hit.v;
    s.roughness = select(m.front.w, 0.6, m.front.w < 0.0);
    s.metal = m.params.x;
    if m.weave.x > 0.5 {
        let spacing = max(m.weave.y, 0.00001);
        let threads = uv / spacing;
        let h = weave_height(threads, m.weave.x);
        color = color * (1.0 - m.weave.z * (1.0 - h.x));
        let t = gradient(a1.w - a0.w, a2.w - a0.w, e1, e2, ng) / spacing;
        let b = gradient(b1.x - b0.x, b2.x - b0.x, e1, e2, ng) / spacing;
        let scale = inverseSqrt(max(max(dot(t, t), dot(b, b)), 1e-20));
        n = normalize(n - 0.35 * m.weave.z * (h.y * t + h.z * b) * scale);
    }
    if m.maps.x > -0.5 {
        let image = sample_map(map_uv, m.maps.x);
        color = color * image.rgb;
        if m.maps.y < 0.0 && image.a < 0.4 {
            s.opacity = 0.0;
        }
        if m.params.w > 0.5 {
            s.opacity = image.a * m.params.z;
        }
        if m.extra.y > -0.5 {
            let o = sample_map(map_uv, m.extra.y);
            s.opacity = s.opacity * o.r * o.a;
        }
        let grad_u = gradient(b1.y - b0.y, b2.y - b0.y, e1, e2, ng);
        let grad_v = gradient(b1.z - b0.z, b2.z - b0.z, e1, e2, ng);
        let texel = 1.0 / vec2f(textureDimensions(maps));
        if m.maps.y > 0.0 {
            let luma = vec3f(0.2126, 0.7152, 0.0722);
            let du = dot(sample_map(map_uv + vec2f(texel.x, 0.0), m.maps.x).rgb - image.rgb, luma);
            let dv = dot(sample_map(map_uv + vec2f(0.0, texel.y), m.maps.x).rgb - image.rgb, luma);
            n = normalize(n - m.maps.y * (du / texel.x * grad_u + dv / texel.y * grad_v));
        }
        if m.maps.z > -0.5 {
            let t = grad_u / max(length(grad_u), 1e-12);
            let b0 = -grad_v / max(length(grad_v), 1e-12);
            let b = normalize(b0 - n * dot(b0, n));
            let mapped = sample_map(map_uv, m.maps.z).rgb * 2.0 - vec3f(1.0);
            n = normalize(t * mapped.x + b * mapped.y + n * max(mapped.z, 0.05));
        } else if m.extra.z > -0.5 && m.params.y > 0.0 {
            // Without a normal map, the displacement map shades its relief.
            let h = sample_map(map_uv, m.extra.z).r;
            let du = sample_map(map_uv + vec2f(texel.x, 0.0), m.extra.z).r - h;
            let dv = sample_map(map_uv + vec2f(0.0, texel.y), m.extra.z).r - h;
            n = normalize(n - m.params.y * (du / texel.x * grad_u + dv / texel.y * grad_v));
        }
        if m.maps.w > -0.5 && s.roughness >= 0.0 {
            s.roughness = sample_map(map_uv, m.maps.w).r;
        }
        if m.extra.x > -0.5 {
            s.metal = sample_map(map_uv, m.extra.x).r;
        }
    }
    // A normal bent below the view would shade light from behind the surface.
    if !finite(n) || dot(n, -direction) <= 0.001 {
        n = s.geometric;
    }
    s.normal = n;
    s.color = color;
    s.metal = clamp(s.metal, 0.0, 1.0);
    s.roughness = clamp(s.roughness, 0.03, 1.0);
    return s;
}

// The environment's direction for an image position, and back; the image's left edge looks
// down -z, its centre down +z... turned about y by the environment's turn.
fn environment_direction(uv: vec2f) -> vec3f {
    let phi = (uv.x - 0.5) * 2.0 * PI + frame.sky_bottom.w;
    let theta = uv.y * PI;
    return vec3f(sin(theta) * sin(phi), cos(theta), -sin(theta) * cos(phi));
}

fn environment_uv(d: vec3f) -> vec2f {
    let phi = atan2(d.x, -d.z) - frame.sky_bottom.w;
    let u = fract(phi / (2.0 * PI) + 0.5);
    let v = acos(clamp(d.y, -1.0, 1.0)) / PI;
    return vec2f(u, v);
}

// Light arriving from direction `d` from the environment.
fn sky(d: vec3f) -> vec3f {
    if frame.options.y > 0.5 {
        let w = frame.size.z;
        let h = frame.size.w;
        let uv = environment_uv(d);
        let x = uv.x * f32(w) - 0.5;
        let y = clamp(uv.y * f32(h) - 0.5, 0.0, f32(h - 1u));
        let x0 = floor(x);
        let y0 = floor(y);
        let fx = x - x0;
        let fy = y - y0;
        let i0 = u32((i32(x0) % i32(w) + i32(w)) % i32(w));
        let i1 = (i0 + 1u) % w;
        let j0 = u32(y0);
        let j1 = min(j0 + 1u, h - 1u);
        let c = mix(
            mix(environment[j0 * w + i0].rgb, environment[j0 * w + i1].rgb, fx),
            mix(environment[j1 * w + i0].rgb, environment[j1 * w + i1].rgb, fx),
            fy,
        );
        return c * frame.sky_top.w;
    }
    let t = smoothstep(-0.3, 0.7, d.y);
    return mix(frame.sky_bottom.rgb, frame.sky_top.rgb, t) * frame.sky_top.w;
}

// Density over directions with which `sample_sky` picks `d`.
fn sky_pdf(d: vec3f) -> f32 {
    if frame.options.y < 0.5 {
        return 0.0;
    }
    let w = frame.size.z;
    let h = frame.size.w;
    let uv = environment_uv(d);
    let i = min(u32(uv.x * f32(w)), w - 1u);
    let j = min(u32(uv.y * f32(h)), h - 1u);
    let row = j * (w + 1u);
    let marginal = h * (w + 1u);
    let p_column = (environment_cdf[row + i + 1u] - environment_cdf[row + i]) * f32(w);
    let p_row = (environment_cdf[marginal + j + 1u] - environment_cdf[marginal + j]) * f32(h);
    let sin_theta = sin(uv.y * PI);
    if sin_theta <= 1e-6 {
        return 0.0;
    }
    return p_column * p_row / (2.0 * PI * PI * sin_theta);
}

// The first index whose cumulative value passes `x`, among `count` cells starting at `base`.
fn search(base: u32, count: u32, x: f32) -> u32 {
    var lo = 0u;
    var hi = count;
    while lo + 1u < hi {
        let mid = (lo + hi) / 2u;
        if environment_cdf[base + mid] <= x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    return lo;
}

// A direction towards the environment, chosen in proportion to its brightness.
fn sample_sky() -> vec3f {
    let w = frame.size.z;
    let h = frame.size.w;
    let j = search(h * (w + 1u), h, random());
    let i = search(j * (w + 1u), w, random());
    let uv = vec2f((f32(i) + random()) / f32(w), (f32(j) + random()) / f32(h));
    return environment_direction(uv);
}

// Whether nothing solid stands between a point and the far distance along `d` (or within
// `t_max`); clear parts of lace and prints let light through.
fn visible(origin: vec3f, d: vec3f, t_max: f32) -> bool {
    var o = origin;
    var left = t_max;
    for (var k = 0; k < 8; k++) {
        let hit = nearest(o, d, left);
        if hit.id == NONE {
            return true;
        }
        let s = surface_at(hit, o, d);
        if random() < s.opacity {
            return false;
        }
        o = s.position + d * EPSILON;
        left -= hit.t;
    }
    return false;
}

fn fresnel(f0: vec3f, cos_theta: f32) -> vec3f {
    return f0 + (vec3f(1.0) - f0) * pow(1.0 - clamp(cos_theta, 0.0, 1.0), 5.0);
}

fn smith_g1(x: f32, a2: f32) -> f32 {
    return 2.0 * x / (x + sqrt(a2 + (1.0 - a2) * x * x));
}

struct Lobe {
    value: vec3f, // BSDF times the cosine at the light
    pdf: f32,     // density over directions with which `sample_bsdf` picks the light
};

// Chance of sampling the specular lobe rather than the diffuse one.
fn specular_chance(s: Surface, cos_view: f32) -> f32 {
    let f0 = mix(vec3f(0.04), s.color, s.metal);
    let spec = luminance(fresnel(f0, cos_view));
    let diffuse = luminance(s.color) * (1.0 - s.metal);
    if diffuse <= 0.0 {
        return 1.0;
    }
    return clamp(spec / (spec + diffuse), 0.15, 0.9);
}

fn evaluate(s: Surface, wo: vec3f, wi: vec3f) -> Lobe {
    let n = s.normal;
    let nl = dot(n, wi);
    let nv = dot(n, wo);
    if nl <= 0.0 || nv <= 0.0 || dot(s.geometric, wi) <= 0.0 {
        return Lobe(vec3f(0.0), 0.0);
    }
    let h = normalize(wo + wi);
    let nh = max(dot(n, h), 0.0);
    let vh = max(dot(wo, h), 0.0);
    let a = s.roughness * s.roughness;
    let a2 = max(a * a, 1e-7);
    let denom = nh * nh * (a2 - 1.0) + 1.0;
    let d = a2 / (PI * denom * denom);
    let f0 = mix(vec3f(0.04), s.color, s.metal);
    let f = fresnel(f0, vh);
    let g1v = smith_g1(nv, a2);
    let specular = d * g1v * smith_g1(nl, a2) * f / (4.0 * nv * nl);
    let diffuse = s.color * (1.0 - s.metal) * (1.0 - fresnel(vec3f(0.04), nv).x) / PI;
    let p_spec = specular_chance(s, nv);
    let pdf = p_spec * d * g1v / (4.0 * nv) + (1.0 - p_spec) * nl / PI;
    return Lobe((specular + diffuse) * nl, pdf);
}

// A direction to continue along, by the BSDF's lobes; the specular one samples the visible
// normals of the GGX distribution (Heitz 2018).
fn sample_bsdf(s: Surface, wo: vec3f) -> vec3f {
    let frame_n = basis(s.normal);
    let local_o = transpose(frame_n) * wo;
    if random() < specular_chance(s, local_o.z) {
        let a = max(s.roughness * s.roughness, 0.001);
        let vh = normalize(vec3f(a * local_o.x, a * local_o.y, local_o.z));
        let length2 = vh.x * vh.x + vh.y * vh.y;
        let t1 = select(vec3f(1.0, 0.0, 0.0), vec3f(-vh.y, vh.x, 0.0) / sqrt(length2), length2 > 0.0);
        let t2 = cross(vh, t1);
        let r = sqrt(random());
        let phi = 2.0 * PI * random();
        let p1 = r * cos(phi);
        let blend = 0.5 * (1.0 + vh.z);
        let p2 = (1.0 - blend) * sqrt(max(1.0 - p1 * p1, 0.0)) + blend * r * sin(phi);
        let nh = p1 * t1 + p2 * t2 + sqrt(max(0.0, 1.0 - p1 * p1 - p2 * p2)) * vh;
        let m = normalize(vec3f(a * nh.x, a * nh.y, max(nh.z, 0.0)));
        return frame_n * reflect(-local_o, m);
    }
    let r = sqrt(random());
    let phi = 2.0 * PI * random();
    let local = vec3f(r * cos(phi), r * sin(phi), sqrt(max(0.0, 1.0 - r * r)));
    return frame_n * local;
}

fn power_heuristic(a: f32, b: f32) -> f32 {
    if a <= 0.0 {
        return 0.0;
    }
    let r = b / a;
    return 1.0 / (1.0 + r * r);
}

// The light a camera ray brings back.
fn radiance(first_origin: vec3f, first_direction: vec3f, pixel: vec2f) -> vec3f {
    var origin = first_origin;
    var direction = first_direction;
    var throughput = vec3f(1.0);
    var light = vec3f(0.0);
    var last_pdf = 0.0;
    var bounce = 0u;
    var passes = 0u;
    let max_bounces = u32(frame.lens.w);
    loop {
        let hit = nearest(origin, direction, 1e30);
        // The viewport fades the floor into the backdrop with distance: so does the camera's
        // first look, while light still bounces off all of it.
        var fade_out = false;
        if hit.id == FLOOR && bounce == 0u {
            let p = origin + direction * hit.t;
            let fade = smoothstep(3.0, 12.0, length(p.xz));
            fade_out = random() < fade;
        }
        if hit.id == NONE || fade_out {
            if bounce == 0u {
                if frame.options.y > 0.5 && frame.options.z > 0.5 {
                    light += throughput * sky(direction);
                } else {
                    light += throughput * mix(frame.backdrop_top.rgb, frame.backdrop_bottom.rgb, pixel.y);
                }
            } else {
                var weight = 1.0;
                if frame.options.y > 0.5 {
                    weight = power_heuristic(last_pdf, sky_pdf(direction));
                }
                light += throughput * sky(direction) * weight;
            }
            break;
        }
        let s = surface_at(hit, origin, direction);
        // Through the clear parts of lace and prints, without counting a bounce.
        if s.opacity < 1.0 && random() >= s.opacity {
            origin = s.position + direction * EPSILON;
            passes += 1u;
            if passes > 16u {
                break;
            }
            continue;
        }
        let wo = -direction;
        let lift = s.position + s.geometric * EPSILON;
        // The sun: a small disc, for soft-edged shadows.
        if dot(frame.sun_color.rgb, vec3f(1.0)) > 0.0 {
            let around = basis(frame.sun.xyz);
            let r = sqrt(random()) * sqrt(max(1.0 - frame.sun.w * frame.sun.w, 0.0));
            let phi = 2.0 * PI * random();
            let wi = normalize(around * vec3f(r * cos(phi), r * sin(phi), 1.0));
            let lobe = evaluate(s, wo, wi);
            if lobe.pdf > 0.0 && visible(lift, wi, 1e30) {
                light += throughput * lobe.value * frame.sun_color.rgb;
            }
        }
        // The environment, sampled by its brightness.
        if frame.options.y > 0.5 {
            let wi = sample_sky();
            let pdf = sky_pdf(wi);
            let lobe = evaluate(s, wo, wi);
            if pdf > 0.0 && lobe.pdf > 0.0 && visible(lift, wi, 1e30) {
                light += throughput * lobe.value * sky(wi) * power_heuristic(pdf, lobe.pdf) / pdf;
            }
        }
        bounce += 1u;
        if bounce > max_bounces {
            break;
        }
        let wi = sample_bsdf(s, wo);
        let lobe = evaluate(s, wo, wi);
        if lobe.pdf <= 0.0 {
            break;
        }
        throughput *= lobe.value / lobe.pdf;
        last_pdf = lobe.pdf;
        // Russian roulette ends dim paths without biasing the mean.
        if bounce > 3u {
            let keep = clamp(max(throughput.x, max(throughput.y, throughput.z)), 0.05, 0.95);
            if random() > keep {
                break;
            }
            throughput /= keep;
        }
        origin = lift;
        direction = wi;
    }
    return light;
}

@compute @workgroup_size(8, 8)
fn trace(@builtin(global_invocation_id) id: vec3u) {
    let width = frame.size.x;
    let height = frame.size.y;
    let x = id.x;
    let y = id.y + frame.pass_info.y;
    if x >= width || y >= height || id.y >= frame.pass_info.z {
        return;
    }
    let index = y * width + x;
    rng = pcg(index ^ pcg(frame.pass_info.x * 9781u + frame.pass_info.w));
    let pixel = (vec2f(f32(x), f32(y)) + vec2f(random(), random())) / vec2f(f32(width), f32(height));
    let ndc = vec2f(pixel.x * 2.0 - 1.0, 1.0 - pixel.y * 2.0);
    var origin = frame.eye.xyz;
    var direction: vec3f;
    if frame.eye.w > 0.5 {
        let half_height = frame.up.w;
        origin += frame.right.xyz * ndc.x * half_height * frame.right.w + frame.up.xyz * ndc.y * half_height;
        direction = frame.forward.xyz;
    } else {
        let t = frame.forward.w;
        direction = normalize(frame.forward.xyz + frame.right.xyz * ndc.x * t * frame.right.w + frame.up.xyz * ndc.y * t);
        // Depth of field: rays leave a disc of the aperture's radius and meet on the plane in focus.
        if frame.lens.x > 0.0 {
            let focus = origin + direction * (frame.lens.y / dot(direction, frame.forward.xyz));
            let r = sqrt(random()) * frame.lens.x;
            let phi = 2.0 * PI * random();
            origin += frame.right.xyz * r * cos(phi) + frame.up.xyz * r * sin(phi);
            direction = normalize(focus - origin);
        }
    }
    var sample = radiance(origin, direction, pixel);
    if !finite(sample) || any(abs(sample) > vec3f(1e20)) {
        atomicAdd(&rejected[0], 1u);
        sample = vec3f(0.0);
    }
    accumulation[index] += vec4f(sample, 1.0);
}

// Filmic tone mapping (Narkowicz's fit of the ACES curve), then sRGB encoding.
fn tone_map(c: vec3f) -> vec3f {
    let x = max(c, vec3f(0.0));
    let mapped = clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), vec3f(0.0), vec3f(1.0));
    return select(1.055 * pow(mapped, vec3f(1.0 / 2.4)) - 0.055, 12.92 * mapped, mapped <= vec3f(0.0031308));
}

@compute @workgroup_size(64)
fn resolve(@builtin(global_invocation_id) id: vec3u) {
    let count = frame.size.x * frame.size.y;
    if id.x >= count {
        return;
    }
    let sum = accumulation[id.x];
    let mean = sum.rgb / max(sum.w, 1.0);
    let c = tone_map(mean * frame.lens.z);
    let bytes = vec4u(vec3u(round(c * 255.0)), 255u);
    display[id.x] = bytes.x | (bytes.y << 8u) | (bytes.z << 16u) | (bytes.w << 24u);
}

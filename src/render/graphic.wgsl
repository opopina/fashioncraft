// Graphics (prints, logos) lying on the cloth: an image sampled across clipped cloth
// triangles, lit like the fabric's diffuse response and blended over it by its alpha.
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

struct VertexOut {
    @builtin(position) position: vec4f,
    @location(0) normal: vec3f,
    @location(1) world: vec3f,
    @location(2) uv: vec2f,
};

@vertex
fn vs_main(@location(0) world: vec3f, @location(1) normal: vec3f, @location(2) uv: vec2f) -> VertexOut {
    var out: VertexOut;
    out.position = view.projection * vec4f(world, 1.0);
    out.normal = normal;
    out.world = world;
    out.uv = uv;
    return out;
}

fn encode_srgb(linear: vec3f) -> vec3f {
    let c = clamp(linear, vec3f(0.0), vec3f(1.0));
    return select(1.055 * pow(c, vec3f(1.0 / 2.4)) - 0.055, 12.92 * c, c <= vec3f(0.0031308));
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4f {
    let texel = textureSample(image, image_sampler, in.uv);
    if texel.a < 0.004 {
        discard;
    }
    let to_eye = view.eye.xyz - in.world * view.eye.w;
    var n = in.normal / max(length(in.normal), 0.000001);
    // Seen from either side the print is lit on the side facing the viewer.
    if dot(n, to_eye) < 0.0 {
        n = -n;
    }
    let nl = clamp(dot(n, normalize(view.light.xyz)), 0.0, 1.0);
    let rgb = texel.rgb * (0.25 * view.ambient.x + 0.7 * view.light.w * nl);
    return vec4f(encode_srgb(rgb), texel.a);
}

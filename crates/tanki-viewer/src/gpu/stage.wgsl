// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//
// The stage's shader. Kept to what WebGL2-class OpenGL can run (uniform buffers, 2D textures, no
// storage buffers), because that is the fallback of old Windows drivers.

struct Globals {
    view_proj: mat4x4<f32>,
    // xyz: the camera's position.
    eye: vec4<f32>,
    // xyz: the direction the key light comes from.
    light: vec4<f32>,
    // xy: centre of the ground shadow, z: ground height, w: its opacity.
    shadow: vec4<f32>,
    // xy: half extents of the ground shadow.
    shadow_size: vec4<f32>,
};

struct Part {
    // xyz: where the part stands.
    offset: vec4<f32>,
    // rgb: the paint under the details, as stored (gamma-encoded).
    paint: vec4<f32>,
    // x: details present, y: lightmap present, z: paint picture present.
    flags: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;

@group(1) @binding(0) var<uniform> part: Part;
@group(1) @binding(1) var details: texture_2d<f32>;
@group(1) @binding(2) var lightmap: texture_2d<f32>;
@group(1) @binding(3) var surface: sampler;
@group(1) @binding(4) var paint: texture_2d<f32>;

struct ModelIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct ModelOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

@vertex
fn vs_model(input: ModelIn) -> ModelOut {
    var out: ModelOut;
    let world = input.position + part.offset.xyz;
    out.clip = globals.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.normal = input.normal;
    out.uv = input.uv;
    return out;
}

@fragment
fn fs_model(input: ModelOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    // Texture reads first, in uniform control flow.
    let detail = textureSample(details, surface, input.uv);
    let baked = textureSample(lightmap, surface, input.uv).rgb;
    let pattern = textureSample(paint, surface, input.uv).rgb;

    // The client blends its textures as stored, gamma-encoded: paint, the details over it where
    // their alpha says, then the lightmap, baked at half brightness (0.5 leaves a colour as it is).
    let under = mix(part.paint.rgb, pattern, part.flags.z);
    var base = mix(under, detail.rgb, detail.a * part.flags.x);
    base = base * mix(vec3<f32>(1.0), baked * 2.0, part.flags.y);
    base = pow(clamp(base, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(2.2));

    // Exports are not always closed or consistently wound: light the side that faces the camera.
    var normal = normalize(input.normal);
    if (!front) {
        normal = -normal;
    }
    let to_eye = normalize(globals.eye.xyz - input.world);
    let key = max(dot(normal, normalize(globals.light.xyz)), 0.0);
    let sky = 0.5 + 0.5 * normal.z;
    let rim = pow(1.0 - max(dot(normal, to_eye), 0.0), 3.0);
    // A soft studio light over the baked one, so the shape reads as it turns.
    let lit = base * (0.85 + 0.3 * key + 0.1 * sky) + vec3<f32>(0.04) * rim;
    return vec4<f32>(lit, 1.0);
}

struct ShadowOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) local: vec2<f32>,
};

@vertex
fn vs_shadow(@builtin(vertex_index) index: u32) -> ShadowOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let corner = corners[index];
    let world = vec3<f32>(
        globals.shadow.xy + corner * globals.shadow_size.xy,
        globals.shadow.z,
    );
    var out: ShadowOut;
    out.clip = globals.view_proj * vec4<f32>(world, 1.0);
    out.local = corner;
    return out;
}

@fragment
fn fs_shadow(input: ShadowOut) -> @location(0) vec4<f32> {
    // A soft dark ellipse under the model, premultiplied: black with the shadow's opacity.
    let fade = 1.0 - smoothstep(0.25, 1.0, length(input.local));
    return vec4<f32>(0.0, 0.0, 0.0, globals.shadow.w * fade * fade);
}

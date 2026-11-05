// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//
// The stage's shader. Kept to what WebGL2-class OpenGL can run (uniform buffers, 2D textures, no
// storage buffers), because that is the fallback of old Windows drivers.

struct Globals {
    view_proj: mat4x4<f32>,
    // xyz: the camera's position.
    eye: vec4<f32>,
    // xyz: the directions the key, fill and rim lights come from.
    key: vec4<f32>,
    fill: vec4<f32>,
    rim: vec4<f32>,
    front: vec4<f32>,
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
    // their alpha says, then the lightmap, baked at half brightness. LIGHTMAP_EXPOSURE and
    // SHADOW_LIFT start from a measurement against the garage pictures the game itself saved in
    // the cache (the mean brightness and the contrast the item cards show), tuned by eye from there
    // on the real models: a little brighter, the shadows a little more lifted.
    let under = mix(part.paint.rgb, pattern, part.flags.z);
    var base = mix(under, detail.rgb, detail.a * part.flags.x);
    base = base * mix(vec3<f32>(1.0), baked * (2.0 * LIGHTMAP_EXPOSURE), part.flags.y);
    base = pow(max(base, vec3<f32>(0.0)), vec3<f32>(2.2));

    // Exports are not always closed or consistently wound: light the side that faces the camera.
    var normal = normalize(input.normal);
    if (!front) {
        normal = -normal;
    }
    let to_eye = normalize(globals.eye.xyz - input.world);
    // The client's own light is baked into the lightmap, which already shades the model as the
    // game does: the studio only hints at the shape over it as the model turns, a little brighter
    // where the key light falls, about 1 on average so the model looks as the game shows it.
    // A soft sky above a darker floor, a key light, a fill, and a rim on the silhouette.
    let hemisphere = mix(GROUND, SKY, 0.5 + 0.5 * normal.z);
    let key = max(dot(normal, globals.key.xyz), 0.0) * KEY;
    let fill = max(dot(normal, globals.fill.xyz), 0.0) * FILL
        + max(dot(normal, globals.front.xyz), 0.0) * FRONT;
    let edge = pow(1.0 - max(dot(normal, to_eye), 0.0), 3.0);
    // A faint neutral rim lifts the silhouette off the dark stage without changing the colours.
    let rim = edge * (0.5 + 0.5 * dot(normal, globals.rim.xyz)) * RIM;
    // The game's garage pictures are flatter than texture × lightmap, their shadows lifted: a
    // gentle curve does the same to the game's colours, and the studio light goes over them.
    let lit = pow(base, vec3<f32>(SHADOW_LIFT)) * (hemisphere + vec3<f32>(key + fill)) + rim;
    return vec4<f32>(neutral(lit), 1.0);
}

const LIGHTMAP_EXPOSURE: f32 = 0.86;
const SHADOW_LIFT: f32 = 0.55;
// The ambient: a sky above, and a floor bright enough that the lower hull and the tracks keep
// their detail (the lightmap keeps the deep shadows of the recesses).
const SKY: vec3<f32> = vec3<f32>(0.94, 0.95, 0.97);
const GROUND: vec3<f32> = vec3<f32>(0.90, 0.90, 0.89);
const KEY: f32 = 0.24;
const FILL: f32 = 0.07;
// About a tenth of the key light: enough to lift the front, not to flatten the shape.
const FRONT: f32 = 0.03;
const RIM: vec3<f32> = vec3<f32>(0.03, 0.03, 0.03);

// The highlight roll-off of Khronos PBR Neutral tone mapping: colours up to 0.8 pass as they are,
// brighter ones roll off toward white instead of clipping, so a lit skin keeps its hue. Its toe,
// which darkens the shadows of a scene in HDR, is left out: the textures are the game's colours
// already, and the toe would take half the brightness of a dark paint.
fn neutral(colour: vec3<f32>) -> vec3<f32> {
    let start = 0.8;
    let desaturation = 0.15;
    var c = max(colour, vec3<f32>(0.0));
    let peak = max(c.r, max(c.g, c.b));
    if (peak < start) {
        return c;
    }
    let d = 1.0 - start;
    let new_peak = 1.0 - d * d / (peak + d - start);
    c = c * (new_peak / peak);
    let g = 1.0 - 1.0 / (desaturation * (peak - new_peak) + 1.0);
    return mix(c, vec3<f32>(new_peak), g);
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
    // A soft contact shadow under the model: a rounded rectangle (a superellipse follows a hull's
    // footprint better than an ellipse), dark under it and fading out past its edges.
    // Premultiplied: black with the shadow's opacity.
    let p = abs(input.local);
    let r = pow(p.x * p.x * p.x * p.x + p.y * p.y * p.y * p.y, 0.25);
    let fade = 1.0 - smoothstep(0.6, 1.0, r);
    return vec4<f32>(0.0, 0.0, 0.0, globals.shadow.w * fade);
}

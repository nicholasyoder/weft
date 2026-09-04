struct Uniforms {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    color: vec4<f32>,
    material: vec4<f32>,
    camera_pos: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> u: Uniforms;

@group(1) @binding(0)
var t_color: texture_2d<f32>;
@group(1) @binding(1)
var s_color: sampler;
// Metallic-roughness texture (Phase 2) — glTF's own channel convention:
// green is roughness, blue is metallic. Reuses `s_color`, same as t_color.
@group(1) @binding(2)
var t_mr: texture_2d<f32>;
// Tangent-space normal map (Phase 3). Reuses `s_color` too.
@group(1) @binding(3)
var t_normal: texture_2d<f32>;

const MAX_LIGHTS: u32 = 4u;

// One light, GPU-side (Phase 4). `pos_or_dir.w`: 0.0 = directional (xyz is
// the direction the light travels), 1.0 = point (xyz is world position).
// `color_intensity.w` is an intensity multiplier. `range.x` is a point
// light's falloff range (unused for directional).
struct GpuLight {
    pos_or_dir: vec4<f32>,
    color_intensity: vec4<f32>,
    range: vec4<f32>,
};

struct Lights {
    lights: array<GpuLight, MAX_LIGHTS>,
    // Light-space view-projection matrix for the scene's shadow-casting
    // light (Phase 5) — meaningless when `shadow_caster_index < 0`.
    shadow_view_proj: mat4x4<f32>,
    count: u32,
    // Index into `lights` of the shadow caster, or -1 if the scene has none.
    shadow_caster_index: i32,
};

@group(2) @binding(0)
var<uniform> lights: Lights;
// Shadow map (Phase 5) — shares this bind group rather than adding a 4th,
// which would exceed wgpu's confirmed default `max_bind_groups` limit on
// the skinned pipeline (already at 4).
@group(2) @binding(1)
var shadow_map: texture_depth_2d;
@group(2) @binding(2)
var shadow_sampler: sampler_comparison;

// Depth bias subtracted from the comparison reference depth, to avoid
// self-shadowing ("shadow acne") at the shadow map's finite resolution.
// Small relative to the [0,1] NDC range spanned by `SHADOW_NEAR`..`SHADOW_FAR`
// (`gpu.rs`) — a bias even a few times larger than this erodes genuinely
// shadowed regions where a grazing light ray only clips a shallow depth of
// the occluder (e.g. near a shadow's silhouette edge).
const SHADOW_BIAS: f32 = 0.0015;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) world_position: vec3<f32>,
    // xyz tangent direction, w handedness — passed through unchanged, see
    // `fs_main`'s TBN construction.
    @location(3) world_tangent: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = u.model * vec4<f32>(in.position, 1.0);
    out.clip_position = u.view_proj * world_pos;
    out.world_normal = (u.model * vec4<f32>(in.normal, 0.0)).xyz;
    out.uv = in.uv;
    out.world_position = world_pos.xyz;
    // A tangent is an ordinary embedded direction, not a normal — it
    // transforms by the model matrix's plain linear part, not the
    // inverse-transpose `world_normal` uses.
    out.world_tangent = vec4<f32>((u.model * vec4<f32>(in.tangent.xyz, 0.0)).xyz, in.tangent.w);
    return out;
}

const PI: f32 = 3.14159265359;

// GGX/Trowbridge-Reitz normal distribution.
fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / max(PI * d * d, 1e-6);
}

// Smith geometry/visibility term (Schlick-GGX approximation), combined for
// both the view and light directions in one call.
fn geometry_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    let ggx_v = n_dot_v / (n_dot_v * (1.0 - k) + k);
    let ggx_l = n_dot_l / (n_dot_l * (1.0 - k) + k);
    return ggx_v * ggx_l;
}

// Schlick's approximation of the Fresnel term.
fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

// Cook-Torrance metallic-roughness BRDF for one light, returning its
// contribution to outgoing radiance (not yet including the light's own
// color/intensity, which the caller multiplies in).
fn shade_light(
    n: vec3<f32>,
    v: vec3<f32>,
    l: vec3<f32>,
    albedo: vec3<f32>,
    roughness: f32,
    metallic: f32,
) -> vec3<f32> {
    let n_dot_l = max(dot(n, l), 0.0);
    if n_dot_l <= 0.0 {
        return vec3<f32>(0.0);
    }
    let h = normalize(v + l);
    let n_dot_v = max(dot(n, v), 1e-4);
    let n_dot_h = max(dot(n, h), 0.0);
    let v_dot_h = max(dot(v, h), 0.0);

    let f0 = mix(vec3<f32>(0.04), albedo, metallic);
    let d = distribution_ggx(n_dot_h, roughness);
    let g = geometry_smith(n_dot_v, n_dot_l, roughness);
    let f = fresnel_schlick(v_dot_h, f0);

    let specular = (d * g * f) / max(4.0 * n_dot_v * n_dot_l, 1e-4);
    let k_diffuse = (vec3<f32>(1.0) - f) * (1.0 - metallic);
    let diffuse = k_diffuse * albedo / PI;

    return (diffuse + specular) * n_dot_l;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let base_color = textureSample(t_color, s_color, in.uv) * u.color;
    let mr = textureSample(t_mr, s_color, in.uv);
    let roughness = clamp(u.material.x * mr.g, 0.045, 1.0);
    let metallic = clamp(u.material.y * mr.b, 0.0, 1.0);

    let geometric_normal = normalize(in.world_normal);
    // Re-orthogonalize after interpolation (interpolating across a
    // triangle's 3 corners doesn't preserve exact perpendicularity), then
    // build the TBN basis and perturb the normal by the tangent-space
    // normal map. The flat-normal default `(0,0,1)` reproduces
    // `geometric_normal` exactly regardless of `tangent`/`bitangent`'s
    // validity (their coefficients are zero in that case).
    let tangent = normalize(
        in.world_tangent.xyz - geometric_normal * dot(geometric_normal, in.world_tangent.xyz)
    );
    let bitangent = cross(geometric_normal, tangent) * in.world_tangent.w;
    let tbn = mat3x3<f32>(tangent, bitangent, geometric_normal);
    let sampled_normal = textureSample(t_normal, s_color, in.uv).rgb * 2.0 - vec3<f32>(1.0);
    let scaled_normal = normalize(vec3<f32>(sampled_normal.xy * u.material.z, sampled_normal.z));
    let n = normalize(tbn * scaled_normal);

    let v = normalize(u.camera_pos.xyz - in.world_position);

    var direct = vec3<f32>(0.0);
    for (var i = 0u; i < lights.count; i++) {
        let light = lights.lights[i];
        var l: vec3<f32>;
        var attenuation = 1.0;
        if light.pos_or_dir.w < 0.5 {
            // Directional: `pos_or_dir.xyz` is the direction the light
            // travels, so the direction *toward* the light is its negation.
            l = normalize(-light.pos_or_dir.xyz);
        } else {
            let to_light = light.pos_or_dir.xyz - in.world_position;
            let dist = max(length(to_light), 1e-4);
            l = to_light / dist;
            let range = max(light.range.x, 1e-4);
            let falloff = clamp(1.0 - pow(dist / range, 4.0), 0.0, 1.0);
            attenuation = (falloff * falloff) / (dist * dist);
        }
        var visibility = 1.0;
        if i32(i) == lights.shadow_caster_index {
            let shadow_clip = lights.shadow_view_proj * vec4<f32>(in.world_position, 1.0);
            let shadow_uv = shadow_clip.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
            visibility = textureSampleCompare(
                shadow_map, shadow_sampler, shadow_uv, shadow_clip.z - SHADOW_BIAS
            );
        }

        let contribution = shade_light(n, v, l, base_color.rgb, roughness, metallic);
        direct +=
            contribution * light.color_intensity.rgb * light.color_intensity.w * attenuation
                * visibility;
    }

    let ambient = 0.15 * base_color.rgb;
    let lit = ambient + direct;
    return vec4<f32>(lit, 1.0);
}

struct Uniforms {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    color: vec4<f32>,
    light_dir: vec4<f32>,
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

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) world_position: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = u.model * vec4<f32>(in.position, 1.0);
    out.clip_position = u.view_proj * world_pos;
    out.world_normal = (u.model * vec4<f32>(in.normal, 0.0)).xyz;
    out.uv = in.uv;
    out.world_position = world_pos.xyz;
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

    let n = normalize(in.world_normal);
    let v = normalize(u.camera_pos.xyz - in.world_position);
    let l = normalize(-u.light_dir.xyz);

    let direct = shade_light(n, v, l, base_color.rgb, roughness, metallic);
    let ambient = 0.15 * base_color.rgb;
    let lit = ambient + direct * 0.85;
    return vec4<f32>(lit, 1.0);
}

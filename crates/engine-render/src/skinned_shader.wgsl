struct Uniforms {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    color: vec4<f32>,
    light_dir: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> u: Uniforms;

@group(1) @binding(0)
var t_color: texture_2d<f32>;
@group(1) @binding(1)
var s_color: sampler;

@group(2) @binding(0)
var<storage, read> joint_matrices: array<mat4x4<f32>>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) joints: vec4<u32>,
    @location(4) weights: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Weighted blend of up to 4 joint matrices — the GPU-skinning step:
    // everything after this point is identical to the unskinned shader,
    // just fed a pre-skinned position/normal instead of the raw mesh one.
    let skin = joint_matrices[in.joints.x] * in.weights.x
        + joint_matrices[in.joints.y] * in.weights.y
        + joint_matrices[in.joints.z] * in.weights.z
        + joint_matrices[in.joints.w] * in.weights.w;

    let skinned_position = skin * vec4<f32>(in.position, 1.0);
    let skinned_normal = skin * vec4<f32>(in.normal, 0.0);

    let world_pos = u.model * skinned_position;
    out.clip_position = u.view_proj * world_pos;
    out.world_normal = (u.model * skinned_normal).xyz;
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let base_color = textureSample(t_color, s_color, in.uv) * u.color;
    let n = normalize(in.world_normal);
    let l = normalize(-u.light_dir.xyz);
    let diffuse = max(dot(n, l), 0.0);
    let ambient = 0.15;
    let lit = base_color.rgb * (ambient + diffuse * 0.85);
    return vec4<f32>(lit, 1.0);
}

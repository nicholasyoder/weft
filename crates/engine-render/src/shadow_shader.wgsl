// Depth-only shadow-map pass (Phase 5) — records the shadow caster's
// light-space depth for every drawable. No `@fragment` entry point at all:
// a depth-only pass needs no color output, and `RenderPipelineDescriptor`'s
// `fragment` field is `Option<FragmentState>`.

struct Uniforms {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    color: vec4<f32>,
    material: vec4<f32>,
    camera_pos: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> u: Uniforms;

@vertex
fn vs_main(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return u.view_proj * u.model * vec4<f32>(position, 1.0);
}

@group(1) @binding(0)
var<storage, read> joint_matrices: array<mat4x4<f32>>;

struct SkinnedVertexInput {
    @location(0) position: vec3<f32>,
    @location(3) joints: vec4<u32>,
    @location(4) weights: vec4<f32>,
};

@vertex
fn vs_main_skinned(in: SkinnedVertexInput) -> @builtin(position) vec4<f32> {
    // Weighted blend of up to 4 joint matrices — identical to
    // `skinned_shader.wgsl`'s own skinning step, just applied to position
    // alone (a depth-only pass has no use for a skinned normal/tangent).
    let skin = joint_matrices[in.joints.x] * in.weights.x
        + joint_matrices[in.joints.y] * in.weights.y
        + joint_matrices[in.joints.z] * in.weights.z
        + joint_matrices[in.joints.w] * in.weights.w;
    let skinned_position = skin * vec4<f32>(in.position, 1.0);
    return u.view_proj * u.model * skinned_position;
}

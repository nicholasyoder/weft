use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::Path;

use glam::{Mat4, Vec3};
use lru::LruCache;
use wgpu::util::DeviceExt;

use crate::components::{Camera, Light, LightKind, Material, MeshKind, MeshRef, Text};
use crate::error::RenderError;
use crate::mesh::{self, SkinnedVertex, Vertex};
use crate::text::{self, GlyphAtlas};
use engine_core::{JointPalette, Transform};

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Fixed capacity for the mesh/texture/skin/font caches below — bounds
/// long-running `engine play`/`--watch` sessions with hot-reloaded assets
/// (previously unbounded `HashMap`s, a real leak risk). Revisit if a real
/// workload needs tuning; no fixture today approaches this many distinct
/// live assets, so eviction never triggers in tests.
const CACHE_CAPACITY: NonZeroUsize = NonZeroUsize::new(256).unwrap();

/// The fallback directional light synthesized when a scene has zero `Light`
/// entities — matches the pre-Phase-4 hardcoded look exactly (see
/// `extract_scene`), so no existing scene file needs updating.
const LIGHT_DIR: Vec3 = Vec3::new(-0.4, -1.0, -0.3);

/// Up to this many `Light` entities may exist in one scene — a small fixed
/// cap comfortably inside the guaranteed 64 KiB uniform binding size, and
/// no concrete scene needs more (see `visual-realism-plan.md` Phase 4).
pub(crate) const MAX_LIGHTS: usize = 4;

/// One light's GPU-side representation, laid out as three `vec4<f32>`s so
/// `array<GpuLight, N>` in a WGSL uniform buffer satisfies naga's
/// 16-byte-stride alignment rule automatically, with no manual padding.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuLight {
    /// xyz: direction the light travels (directional) or world-space
    /// position (point). w: 0.0 = directional, 1.0 = point — the shader
    /// reads this tag to pick which falloff to apply.
    pos_or_dir: [f32; 4],
    /// rgb: light color. w: intensity multiplier.
    color_intensity: [f32; 4],
    /// x: point-light range (unused for directional). yzw: padding, kept
    /// explicit so every field of this struct is a whole 16-byte-aligned
    /// vec4.
    range: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Lights {
    lights: [GpuLight; MAX_LIGHTS],
    /// The shadow caster's light-space view-projection matrix (Phase 5) —
    /// meaningless when `shadow_caster_index < 0`.
    shadow_view_proj: [[f32; 4]; 4],
    count: u32,
    /// Index into `lights` of the scene's shadow-casting light, or `-1` if
    /// none. `extract_scene` guarantees at most one.
    shadow_caster_index: i32,
    _pad: [u32; 2],
}

/// Fixed-resolution square shadow map — independent of the output
/// width/height a given `draw()` call renders at, so (unlike the main depth
/// buffer) it's built once in `RenderContext::from_core`, not per frame.
const SHADOW_MAP_SIZE: u32 = 2048;
/// Half-width/height of the shadow caster's orthographic frustum, centered
/// on the main camera's look-at target. Not scene-bounds-fitted or
/// cascaded — a deliberate, named scoping limit (see Phase 5). Bumped from
/// 10.0 to cover `games/sandbox`'s enlarged ~34x34 arena (corner-to-center
/// distance ~24) when that scene outgrew the original 20x20 fit — still a
/// fixed constant, not adaptive; ADR-0019's "revisit when a real level's
/// geometry outgrows this" note is about a bigger structural change
/// (scene-bounds-fitted/cascaded volumes), not this kind of bump.
const SHADOW_ORTHO_HALF_EXTENT: f32 = 26.0;
const SHADOW_NEAR: f32 = 0.1;
const SHADOW_FAR: f32 = 60.0;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    model: [[f32; 4]; 4],
    color: [f32; 4],
    /// `[roughness, metallic, 0.0, 0.0]` — kept as its own vec4 rather than
    /// repurposing `color`'s unused alpha channel, so `material.x`/`.y` read
    /// clearly in the shader as separate PBR scalars, not a smuggled alpha.
    material: [f32; 4],
    /// World-space camera position (`.w` unused) — needed to build the view
    /// vector the Cook-Torrance specular term reads, which a flat-Lambertian
    /// shader never needed.
    camera_pos: [f32; 4],
}

struct Drawable {
    /// Identifies this drawable's ECS entity across frames — the key
    /// `draw()`'s per-drawable buffer/bind-group pools (Phase 6) use to
    /// reuse a stable GPU allocation instead of creating a fresh one every
    /// frame.
    entity: hecs::Entity,
    transform: Transform,
    mesh: MeshKind,
    color: [f32; 3],
    roughness: f32,
    metallic: f32,
    texture: Option<String>,
    /// Content hash of a texture whose G channel modulates `roughness` and
    /// B channel modulates `metallic` per-pixel (glTF's own channel
    /// convention for `metallicRoughnessTexture`) — see Phase 2 /
    /// ADR-0019. `None` renders identically to Phase 1 (scalars only).
    metallic_roughness_texture: Option<String>,
    /// Content hash of a tangent-space normal map — see Phase 3 / ADR-0019.
    /// `None` renders identically to Phase 2 (the flat-normal default, a
    /// mathematically exact no-op).
    normal_texture: Option<String>,
    normal_scale: f32,
    /// A content hash into the `SkinData` asset store, plus the same
    /// entity's `JointPalette` matrices (if any) — both must be present to
    /// actually draw skinned; see `draw()`'s `use_skinned` check for why a
    /// `skin` with no palette yet falls back to an ordinary static draw
    /// rather than erroring or guessing a joint count.
    skin: Option<String>,
    joint_matrices: Option<Vec<[[f32; 4]; 4]>>,
    /// Content hash into the `TangentData` asset store — `None` for any
    /// mesh with no normal map assigned (see `MeshRef.tangent`'s doc
    /// comment).
    tangent: Option<String>,
}

struct TextDrawable {
    content: String,
    x: f32,
    y: f32,
    size: f32,
    color: [f32; 4],
    font: Option<String>,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ScreenUniform {
    size: [f32; 2],
    _pad: [f32; 2],
}

type MeshBuffers = (wgpu::Buffer, wgpu::Buffer, u32);

/// Instance/adapter/device/queue — the part of the GPU setup that a live
/// windowed loop and a one-shot offscreen render share identically.
/// Vulkan/Metal/DX12 (this engine's only targeted backends, per
/// ADR-0004/ADR-0010) don't need `InstanceDescriptor::display` pre-set —
/// `Instance::create_surface` derives both window and display handles from
/// the window value itself (wgpu's `SurfaceTarget::DisplayAndWindow`), so
/// headless and windowed instance construction share this one path; only
/// `compatible_surface` on the adapter request differs.
pub(crate) struct GraphicsCore {
    // Kept alive even though nothing reads it after adapter creation, out
    // of caution: an adapter/device are generally expected to be used
    // alongside the instance that created them, and keeping it costs
    // nothing. (The actual root cause of a SIGSEGV hit while debugging this
    // phase turned out to be unrelated — see the `InstanceFlags::empty()`
    // comment in `make_instance` and ADR-0010 — but there's no reason to
    // reintroduce an early drop this crate never needed in the first
    // place.)
    #[allow(dead_code)]
    instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl GraphicsCore {
    pub(crate) fn new_headless(backends: wgpu::Backends) -> Result<Self, RenderError> {
        let instance = Self::make_instance(backends);
        pollster::block_on(Self::request(instance, None))
    }

    /// Builds a core plus a `Surface` for `window`. Returns the surface
    /// separately (rather than storing it here) since `RenderContext` — the
    /// type built from this core — has no notion of a presentation target;
    /// only `WindowRenderer` (which owns this surface) does.
    pub(crate) fn new_windowed(
        window: std::sync::Arc<winit::window::Window>,
        backends: wgpu::Backends,
    ) -> Result<(Self, wgpu::Surface<'static>), RenderError> {
        let instance = Self::make_instance(backends);
        let surface = instance
            .create_surface(window)
            .map_err(|e| RenderError::SurfaceCreateFailed(e.to_string()))?;
        let core = pollster::block_on(Self::request(instance, Some(&surface)))?;
        Ok((core, surface))
    }

    fn make_instance(backends: wgpu::Backends) -> wgpu::Instance {
        wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            // Explicitly no DEBUG/VALIDATION (wgpu's debug-build default,
            // via `InstanceFlags::from_build_config`) — with it on, wgpu-hal
            // tags every resource via `VK_EXT_debug_utils`
            // `set_object_name`, which was root-caused (via gdb) to
            // segfault inside `libvulkan.so.1` under concurrent Vulkan
            // instance creation from multiple threads (this project's own
            // test suite; see ADR-0010). Not needed at runtime regardless.
            flags: wgpu::InstanceFlags::empty(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        })
    }

    async fn request(
        instance: wgpu::Instance,
        compatible_surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<Self, RenderError> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::None,
                compatible_surface,
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .map_err(|e| RenderError::AdapterRequestFailed(e.to_string()))?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("weft-render-device"),
                ..Default::default()
            })
            .await
            .map_err(|e| RenderError::DeviceRequestFailed(e.to_string()))?;

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
        })
    }
}

/// Everything device-dependent but target-independent: shader, bind group
/// layouts, a render-pipeline cache (keyed by color target format, since an
/// offscreen render and a windowed surface may use different formats), and
/// every mesh/texture/sampler resource — built once and reused across
/// frames. Persisting this (instead of rebuilding it on every call, as the
/// pre-Phase-8 code did) is what makes a real-time loop viable: mesh and
/// texture uploads stop happening every single frame.
pub struct RenderContext {
    // Rust drops struct fields in declaration order (top to bottom) — the
    // opposite of local-variable drop order (reverse declaration), which is
    // what the pre-Phase-8 single-function `render()` relied on implicitly
    // (device/instance declared first, so dropped last). `core` is
    // declared *last* here purely as a defensive precaution so every GPU
    // resource below (which depends on `core.device`) is still dropped
    // before it, matching that same safe ordering — not a confirmed fix for
    // any specific bug (the actual SIGSEGV root-caused during this phase
    // was the `set_object_name`/debug-labels issue fixed in
    // `make_instance`; see ADR-0010), just no reason to risk the opposite.
    shader: wgpu::ShaderModule,
    bind_group_layout: wgpu::BindGroupLayout,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    sampler: wgpu::Sampler,
    // Shared 1x1 white view — the "no texture here" default for *any* of
    // the texture bind group's slots (base color, and since Phase 2,
    // metallic-roughness too): white is simultaneously a no-op color tint
    // and a no-op rough/metal multiplier, so one texture covers both.
    white_view: wgpu::TextureView,
    // Shared 1x1 flat-tangent-space-normal view `(128,128,255)` — decodes to
    // `(0,0,1)`, the "no perturbation" default for the normal-map slot
    // (Phase 3). Plain white would decode to an out-of-range direction, so
    // this can't reuse `white_view`.
    flat_normal_view: wgpu::TextureView,
    white_bind_group: wgpu::BindGroup,
    cube_buffers: MeshBuffers,
    plane_buffers: MeshBuffers,
    sphere_buffers: MeshBuffers,
    // Keyed by (mesh_hash, tangent_hash) — Phase 3 lets a `MeshRef`
    // reference an independent tangent-data asset, so the bare mesh hash no
    // longer uniquely identifies the vertex buffer a drawable needs (same
    // reasoning `texture_cache`/`skin_cache` already apply).
    mesh_cache: LruCache<(String, Option<String>), MeshBuffers>,
    // Keyed by (base_color_hash, metallic_roughness_hash) — Phase 2 lets a
    // material reference two independent textures, so either hash alone no
    // longer uniquely identifies the bind group a drawable needs.
    texture_cache: LruCache<(String, Option<String>, Option<String>), wgpu::BindGroup>,
    // Skinned mesh pass — a second shader/pipeline (opaque, same depth
    // settings as the 3D pipeline; not alpha-blended like the UI pass
    // below) with a third bind group carrying a per-draw joint-matrix
    // storage buffer. See ADR-0015.
    skinned_shader: wgpu::ShaderModule,
    joint_bind_group_layout: wgpu::BindGroupLayout,
    skinned_pipeline_layout: wgpu::PipelineLayout,
    skinned_pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    // Keyed by (mesh_hash, skin_hash) — a mesh could in principle be
    // referenced with different skins, though `engine import` never
    // produces that today.
    skin_cache: LruCache<(String, String, Option<String>), MeshBuffers>,
    // Per-entity pools (Phase 6) replacing what used to be a fresh
    // `create_buffer_init` + `create_bind_group` per drawable every single
    // `draw()` call — an existing entity's buffer is now updated in place
    // via `queue.write_buffer`, and an entity no longer present in the
    // current frame's drawable set is evicted at the end of `draw()`. Only
    // pays off for the live/windowed path (a stable entity set across
    // frames); one-shot/batch callers just don't get a "next frame" to
    // amortize across, same caveat as every other cache in this file.
    // `joint_pool` is shared by both the main pass and the shadow pass —
    // both fill it from the exact same `JointPalette` matrices, so reusing
    // one entry rather than keeping two (as the pre-Phase-6 code did)
    // removes a redundant allocation, not just redundant bookkeeping.
    uniform_pool: HashMap<hecs::Entity, (wgpu::Buffer, wgpu::BindGroup)>,
    shadow_uniform_pool: HashMap<hecs::Entity, (wgpu::Buffer, wgpu::BindGroup)>,
    joint_pool: HashMap<hecs::Entity, (wgpu::Buffer, wgpu::BindGroup)>,
    // UI/text pass — a second shader/pipeline (alpha-blended, depth-always)
    // reusing `texture_bind_group_layout`/`sampler` above for its glyph
    // atlas textures, since an R8Unorm atlas fits that same layout shape.
    // See ADR-0014.
    ui_shader: wgpu::ShaderModule,
    ui_bind_group_layout: wgpu::BindGroupLayout,
    ui_pipeline_layout: wgpu::PipelineLayout,
    ui_pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    default_atlas: GlyphAtlas,
    font_cache: LruCache<String, GlyphAtlas>,
    // Lights (Phase 4) — one shared bind group carrying every light in the
    // scene. Shared by both the plain and skinned pipelines. Since Phase 5
    // this layout also carries the shadow map texture/sampler (bindings
    // 1/2) — adding a fourth bind group for them would exceed wgpu's
    // confirmed default `max_bind_groups` limit on the skinned pipeline,
    // already at 4. Built once here, not per `draw()` call (Phase 6) —
    // `shadow_map_view` was already stable across frames, and
    // `lights_buffer` now is too (updated via `queue.write_buffer` each
    // frame instead of recreated), so the bind group itself never needs to
    // change shape frame to frame.
    lights_buffer: wgpu::Buffer,
    lights_bind_group: wgpu::BindGroup,
    // Shadow map render pass (Phase 5) — depth-only, fixed resolution
    // (`SHADOW_MAP_SIZE`), so unlike `pipelines`/`skinned_pipelines` above
    // this doesn't need a per-color-format `HashMap`: built once here, not
    // per `draw()` call (the shadow map's size never depends on the output
    // surface's width/height the way the main depth buffer does). The
    // shader/pipeline-layouts that build these pipelines aren't kept —
    // unlike `pipeline_layout`, nothing rebuilds a shadow pipeline lazily
    // per format, so once built there's no further use for them.
    shadow_pipeline: wgpu::RenderPipeline,
    skinned_shadow_pipeline: wgpu::RenderPipeline,
    // Not a struct field: unlike `shadow_map_view` (read every frame in
    // `draw()`), the sampler is only needed once, up front, to build
    // `lights_bind_group` — wgpu's bind group holds its own internal
    // refcount on the sampler resource, so nothing here needs to keep a
    // Rust-level handle to it alive afterward.
    shadow_map_view: wgpu::TextureView,
    core: GraphicsCore,
}

fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x3,
            },
            wgpu::VertexAttribute {
                offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x3,
            },
            wgpu::VertexAttribute {
                offset: (std::mem::size_of::<[f32; 3]>() * 2) as wgpu::BufferAddress,
                shader_location: 2,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                offset: (std::mem::size_of::<[f32; 3]>() * 2 + std::mem::size_of::<[f32; 2]>())
                    as wgpu::BufferAddress,
                shader_location: 3,
                format: wgpu::VertexFormat::Float32x4,
            },
        ],
    }
}

fn skinned_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const JOINTS_OFFSET: wgpu::BufferAddress = (std::mem::size_of::<[f32; 3]>() * 2
        + std::mem::size_of::<[f32; 2]>())
        as wgpu::BufferAddress;
    const TANGENT_OFFSET: wgpu::BufferAddress = JOINTS_OFFSET
        + std::mem::size_of::<[u32; 4]>() as wgpu::BufferAddress
        + std::mem::size_of::<[f32; 4]>() as wgpu::BufferAddress;
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<SkinnedVertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x3,
            },
            wgpu::VertexAttribute {
                offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x3,
            },
            wgpu::VertexAttribute {
                offset: (std::mem::size_of::<[f32; 3]>() * 2) as wgpu::BufferAddress,
                shader_location: 2,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                offset: JOINTS_OFFSET,
                shader_location: 3,
                format: wgpu::VertexFormat::Uint32x4,
            },
            wgpu::VertexAttribute {
                offset: JOINTS_OFFSET + std::mem::size_of::<[u32; 4]>() as wgpu::BufferAddress,
                shader_location: 4,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: TANGENT_OFFSET,
                shader_location: 5,
                format: wgpu::VertexFormat::Float32x4,
            },
        ],
    }
}

/// Shared by `pipeline_for`/`skinned_pipeline_for`/`ui_pipeline_for` — those
/// three `RenderPipelineDescriptor`s are ~95% identical, differing only in
/// the params below. Everything else (entry points, `compilation_options`,
/// `write_mask`, `topology`/`front_face`, `depth_stencil.format`,
/// `stencil`/`bias` defaults, `multisample`/`multiview_mask`/`cache`) is the
/// same across all three call sites.
#[allow(clippy::too_many_arguments)]
fn build_pipeline(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    vertex_buffer_layout: wgpu::VertexBufferLayout,
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
    cull_mode: Option<wgpu::Face>,
    depth_write_enabled: bool,
    depth_compare: wgpu::CompareFunction,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[Some(vertex_buffer_layout)],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(depth_write_enabled),
            depth_compare: Some(depth_compare),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

/// Depth-only twin of `build_pipeline` (Phase 5): no fragment stage at all
/// (`fragment: None` — valid per `RenderPipelineDescriptor`, confirmed for a
/// pass with no color attachments), so it can't share `build_pipeline`
/// itself without several dead parameters (format/blend/cull_mode all stop
/// applying to a shadow pass).
fn build_shadow_pipeline(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    entry_point: &str,
    vertex_buffer_layout: wgpu::VertexBufferLayout,
    bias: wgpu::DepthBiasState,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(entry_point),
            buffers: &[Some(vertex_buffer_layout)],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: None,
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias,
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

/// Looks up `entity` in `pool`, updating its buffer's contents in place via
/// `queue.write_buffer` when it already exists and is the right size, or
/// creating a fresh buffer+bind-group (mirroring the pre-Phase-6
/// `create_buffer_init`+`create_bind_group` shape exactly) when it doesn't —
/// the one shared mechanism `draw()`'s per-drawable `Uniforms` pools
/// (shadow-pass and main-pass) and its skinned joint-matrix pool all use.
/// A size mismatch is never expected in practice (an entity's uniform
/// struct size is fixed at compile time; a skinned entity's joint count is
/// fixed by its skin, which doesn't change frame to frame) but falls back
/// to a fresh allocation rather than risking a `write_buffer` panic.
#[allow(clippy::too_many_arguments)]
fn write_pooled_buffer<'a>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    pool: &'a mut HashMap<hecs::Entity, (wgpu::Buffer, wgpu::BindGroup)>,
    entity: hecs::Entity,
    label: &str,
    contents: &[u8],
    usage: wgpu::BufferUsages,
) -> &'a wgpu::BindGroup {
    let needs_new = match pool.get(&entity) {
        Some((buffer, _)) => buffer.size() != contents.len() as u64,
        None => true,
    };
    if needs_new {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents,
            usage: usage | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        pool.insert(entity, (buffer, bind_group));
    } else {
        let (buffer, _) = pool.get(&entity).unwrap();
        queue.write_buffer(buffer, 0, contents);
    }
    &pool.get(&entity).unwrap().1
}

impl RenderContext {
    pub fn new_headless(backends: wgpu::Backends) -> Result<Self, RenderError> {
        Self::from_core(GraphicsCore::new_headless(backends)?)
    }

    pub(crate) fn from_core(core: GraphicsCore) -> Result<Self, RenderError> {
        let device = &core.device;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("weft-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("weft-uniform-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("weft-texture-layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    // Metallic-roughness texture (Phase 2) — reuses the
                    // same sampler at binding 1, glTF's own base-color and
                    // metallic-roughness textures are required to be
                    // sampler-compatible, and this engine already has
                    // exactly one shared sampler.
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // Normal map (Phase 3) — reuses the same shared sampler
                    // too, same reasoning as binding 2 above.
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });

        // Lights (Phase 4) — one uniform buffer entry, fragment-only (no
        // vertex-stage light math). Shared by the plain and skinned
        // pipelines; the skinned pipeline lands at exactly 4 bind groups
        // with this added, wgpu's confirmed default `max_bind_groups`. Since
        // Phase 5 this same group also carries the shadow map texture
        // (binding 1) and its comparison sampler (binding 2) — a 5th bind
        // group would exceed that same limit.
        let lights_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("weft-lights-layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                        count: None,
                    },
                ],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("weft-pipeline-layout"),
            bind_group_layouts: &[
                Some(&bind_group_layout),
                Some(&texture_bind_group_layout),
                Some(&lights_bind_group_layout),
            ],
            immediate_size: 0,
        });

        let skinned_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("weft-skinned-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("skinned_shader.wgsl").into()),
        });

        let joint_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("weft-joint-layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let skinned_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("weft-skinned-pipeline-layout"),
                bind_group_layouts: &[
                    Some(&bind_group_layout),
                    Some(&texture_bind_group_layout),
                    Some(&joint_bind_group_layout),
                    Some(&lights_bind_group_layout),
                ],
                immediate_size: 0,
            });

        // Shadow map render pass (Phase 5). `shadow_pipeline_layout` reuses
        // `bind_group_layout` as-is (group 0) — the shadow pass fills the
        // same per-drawable `Uniforms` buffer with the light's `view_proj`
        // instead of the camera's, needing no new bind-group-layout shape.
        let shadow_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("weft-shadow-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shadow_shader.wgsl").into()),
        });
        let shadow_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("weft-shadow-pipeline-layout"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            });
        let skinned_shadow_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("weft-skinned-shadow-pipeline-layout"),
                bind_group_layouts: &[Some(&bind_group_layout), Some(&joint_bind_group_layout)],
                immediate_size: 0,
            });
        // No rasterization-level depth bias here — deliberately. A
        // shadow-pass `DepthBiasState` shifts every occluder's *stored*
        // depth uniformly (in NDC units, translated to a comparatively huge
        // world-space offset by this pass's wide `SHADOW_FAR`/`SHADOW_NEAR`
        // range), which erodes genuinely-shadowed regions where a grazing
        // light ray only clips a shallow depth of the occluder (confirmed
        // empirically: even wgpu's small default-scale bias values washed
        // out most of a test shadow's silhouette). `fs_main`'s own
        // `SHADOW_BIAS` — a fixed, small comparison-depth offset applied
        // once at the receiver, not compounded per rasterized occluder
        // fragment — is the only bias needed against acne here.
        let shadow_bias = wgpu::DepthBiasState::default();
        let shadow_pipeline = build_shadow_pipeline(
            device,
            "weft-shadow-pipeline",
            &shadow_pipeline_layout,
            &shadow_shader,
            "vs_main",
            vertex_layout(),
            shadow_bias,
        );
        let skinned_shadow_pipeline = build_shadow_pipeline(
            device,
            "weft-skinned-shadow-pipeline",
            &skinned_shadow_pipeline_layout,
            &shadow_shader,
            "vs_main_skinned",
            skinned_vertex_layout(),
            shadow_bias,
        );
        let shadow_map = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("weft-shadow-map"),
            size: wgpu::Extent3d {
                width: SHADOW_MAP_SIZE,
                height: SHADOW_MAP_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_map_view = shadow_map.create_view(&wgpu::TextureViewDescriptor::default());
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("weft-shadow-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });

        let cube_buffers = upload_mesh(device, &mesh::cube());
        let plane_buffers = upload_mesh(device, &mesh::plane());
        let sphere_buffers = upload_mesh(device, &mesh::sphere());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("weft-sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        // Untextured drawables sample a shared 1x1 white texture rather than
        // branching in the shader — white * color reproduces the flat-color
        // look exactly, and white as a rough/metal multiplier is likewise a
        // no-op, so one shader code path covers both cases.
        let white_view = upload_white_texture_view(device, &core.queue);
        let flat_normal_view = upload_flat_normal_view(device, &core.queue);
        let white_bind_group = create_white_texture_bind_group(
            device,
            &texture_bind_group_layout,
            &sampler,
            &white_view,
            &flat_normal_view,
        );

        let ui_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("weft-ui-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("ui_shader.wgsl").into()),
        });

        let ui_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("weft-ui-uniform-layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let ui_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("weft-ui-pipeline-layout"),
            bind_group_layouts: &[
                Some(&ui_bind_group_layout),
                Some(&texture_bind_group_layout),
            ],
            immediate_size: 0,
        });

        let default_atlas = GlyphAtlas::build(
            device,
            &core.queue,
            &texture_bind_group_layout,
            &sampler,
            text::DEFAULT_FONT_BYTES,
        )?;

        // Built once, not per `draw()` call (Phase 6) — sized for the
        // `Lights` struct up front and filled via `queue.write_buffer` every
        // frame from here on, same as every other per-frame GPU resource
        // this phase pools.
        let lights_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("weft-lights-uniforms"),
            size: std::mem::size_of::<Lights>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let lights_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("weft-lights-bind-group"),
            layout: &lights_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: lights_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&shadow_map_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
            ],
        });

        Ok(Self {
            core,
            shader,
            bind_group_layout,
            texture_bind_group_layout,
            pipeline_layout,
            pipelines: HashMap::new(),
            sampler,
            white_view,
            flat_normal_view,
            white_bind_group,
            cube_buffers,
            plane_buffers,
            sphere_buffers,
            mesh_cache: LruCache::new(CACHE_CAPACITY),
            texture_cache: LruCache::new(CACHE_CAPACITY),
            skinned_shader,
            joint_bind_group_layout,
            skinned_pipeline_layout,
            skinned_pipelines: HashMap::new(),
            skin_cache: LruCache::new(CACHE_CAPACITY),
            uniform_pool: HashMap::new(),
            shadow_uniform_pool: HashMap::new(),
            joint_pool: HashMap::new(),
            ui_shader,
            ui_bind_group_layout,
            ui_pipeline_layout,
            ui_pipelines: HashMap::new(),
            default_atlas,
            font_cache: LruCache::new(CACHE_CAPACITY),
            lights_buffer,
            lights_bind_group,
            shadow_pipeline,
            skinned_shadow_pipeline,
            shadow_map_view,
        })
    }

    pub(crate) fn device(&self) -> &wgpu::Device {
        &self.core.device
    }

    pub(crate) fn queue(&self) -> &wgpu::Queue {
        &self.core.queue
    }

    fn pipeline_for(&mut self, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
        let device = &self.core.device;
        let shader = &self.shader;
        let pipeline_layout = &self.pipeline_layout;
        self.pipelines
            .entry(format)
            .or_insert_with(|| {
                build_pipeline(
                    device,
                    "weft-pipeline",
                    pipeline_layout,
                    shader,
                    vertex_layout(),
                    format,
                    None,
                    Some(wgpu::Face::Back),
                    true,
                    wgpu::CompareFunction::Less,
                )
            })
            .clone()
    }

    /// The skinned-mesh pipeline variant: same opaque/depth-tested shape as
    /// `pipeline_for`, just a different shader, vertex layout, and a third
    /// (joint-matrix storage buffer) bind group — see `skinned_shader.wgsl`.
    fn skinned_pipeline_for(&mut self, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
        let device = &self.core.device;
        let shader = &self.skinned_shader;
        let pipeline_layout = &self.skinned_pipeline_layout;
        self.skinned_pipelines
            .entry(format)
            .or_insert_with(|| {
                build_pipeline(
                    device,
                    "weft-skinned-pipeline",
                    pipeline_layout,
                    shader,
                    skinned_vertex_layout(),
                    format,
                    None,
                    Some(wgpu::Face::Back),
                    true,
                    wgpu::CompareFunction::Less,
                )
            })
            .clone()
    }

    /// The UI/text pipeline variant: alpha-blended (the 3D pipeline never
    /// blends — `blend: None` there), and `depth_compare: Always` with
    /// writes disabled so HUD text always draws on top of the 3D scene
    /// regardless of what's in the shared depth buffer, without needing a
    /// second render pass.
    fn ui_pipeline_for(&mut self, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
        let device = &self.core.device;
        let shader = &self.ui_shader;
        let pipeline_layout = &self.ui_pipeline_layout;
        self.ui_pipelines
            .entry(format)
            .or_insert_with(|| {
                build_pipeline(
                    device,
                    "weft-ui-pipeline",
                    pipeline_layout,
                    shader,
                    text::ui_vertex_layout(),
                    format,
                    Some(wgpu::BlendState::ALPHA_BLENDING),
                    None,
                    false,
                    wgpu::CompareFunction::Always,
                )
            })
            .clone()
    }

    /// Records one frame's render pass into `encoder` against `color_view`
    /// (of `color_format`) and `depth_view`. Doesn't submit or present —
    /// that's the caller's job, since the offscreen path needs to append a
    /// copy-to-buffer step to the same encoder before submitting, while the
    /// windowed path just submits and presents.
    #[allow(clippy::too_many_arguments)]
    fn draw(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        drawables: &[Drawable],
        texts: &[TextDrawable],
        view_proj: Mat4,
        camera_pos: Vec3,
        lights: &Lights,
        color_view: &wgpu::TextureView,
        color_format: wgpu::TextureFormat,
        depth_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        assets_dir: &Path,
    ) -> Result<(), RenderError> {
        let pipeline = self.pipeline_for(color_format);
        let skinned_pipeline = self.skinned_pipeline_for(color_format);
        let ui_pipeline = self.ui_pipeline_for(color_format);
        let asset_store = engine_assets::AssetStore::new(assets_dir);

        // Shadow map pass (Phase 5) — recorded first, into `self.shadow_map_view`,
        // so the main pass below can sample it. Skipped (map left cleared to
        // 1.0 / "unoccluded") when the scene has no shadow caster, keeping a
        // zero-shadow-caster scene's GPU work close to what it was before
        // this phase.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("weft-shadow-pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_map_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            if lights.shadow_caster_index >= 0 {
                for drawable in drawables {
                    let uniforms = Uniforms {
                        view_proj: lights.shadow_view_proj,
                        model: drawable.transform.to_matrix().to_cols_array_2d(),
                        color: [0.0; 4],
                        material: [0.0; 4],
                        camera_pos: [0.0; 4],
                    };
                    let bind_group = write_pooled_buffer(
                        &self.core.device,
                        &self.core.queue,
                        &self.bind_group_layout,
                        &mut self.shadow_uniform_pool,
                        drawable.entity,
                        "weft-shadow-draw-uniforms",
                        bytemuck::bytes_of(&uniforms),
                        wgpu::BufferUsages::UNIFORM,
                    );

                    let skinned = match (&drawable.mesh, &drawable.skin, &drawable.joint_matrices) {
                        (MeshKind::Asset(mesh_hash), Some(skin_hash), Some(matrices)) => Some((
                            mesh_hash.clone(),
                            skin_hash.clone(),
                            drawable.tangent.clone(),
                            matrices,
                        )),
                        _ => None,
                    };

                    if let Some((mesh_hash, skin_hash, tangent_hash, matrices)) = skinned {
                        let key = (mesh_hash, skin_hash, tangent_hash);
                        if !self.skin_cache.contains(&key) {
                            let buffers = load_skinned_mesh_buffers(
                                &key.0,
                                &key.1,
                                key.2.as_deref(),
                                &self.core.device,
                                &asset_store,
                            )?;
                            self.skin_cache.put(key.clone(), buffers);
                        }
                        let (vertex_buffer, index_buffer, index_count) =
                            self.skin_cache.get(&key).unwrap();

                        // Shared with the main pass below (Phase 6) — both
                        // fill the same entity's slot from the identical
                        // `matrices`, so whichever pass runs first creates
                        // it and the other just reuses/rewrites it, rather
                        // than each pass keeping its own copy.
                        let joint_bind_group = write_pooled_buffer(
                            &self.core.device,
                            &self.core.queue,
                            &self.joint_bind_group_layout,
                            &mut self.joint_pool,
                            drawable.entity,
                            "weft-joint-matrices",
                            bytemuck::cast_slice(matrices.as_slice()),
                            wgpu::BufferUsages::STORAGE,
                        );

                        pass.set_pipeline(&self.skinned_shadow_pipeline);
                        pass.set_bind_group(0, bind_group, &[]);
                        pass.set_bind_group(1, joint_bind_group, &[]);
                        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..*index_count, 0, 0..1);
                        continue;
                    }

                    let (vertex_buffer, index_buffer, index_count) = match &drawable.mesh {
                        MeshKind::Cube => (
                            &self.cube_buffers.0,
                            &self.cube_buffers.1,
                            self.cube_buffers.2,
                        ),
                        MeshKind::Plane => (
                            &self.plane_buffers.0,
                            &self.plane_buffers.1,
                            self.plane_buffers.2,
                        ),
                        MeshKind::Sphere => (
                            &self.sphere_buffers.0,
                            &self.sphere_buffers.1,
                            self.sphere_buffers.2,
                        ),
                        MeshKind::Asset(hash) => {
                            let key = (hash.clone(), drawable.tangent.clone());
                            if !self.mesh_cache.contains(&key) {
                                let buffers = load_mesh_buffers(
                                    hash,
                                    drawable.tangent.as_deref(),
                                    &self.core.device,
                                    &asset_store,
                                )?;
                                self.mesh_cache.put(key.clone(), buffers);
                            }
                            let (vb, ib, count) = self.mesh_cache.get(&key).unwrap();
                            (vb, ib, *count)
                        }
                    };

                    pass.set_pipeline(&self.shadow_pipeline);
                    pass.set_bind_group(0, bind_group, &[]);
                    pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                    pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..index_count, 0, 0..1);
                }
            }
        }

        // `lights_buffer`/`lights_bind_group` are both built once in
        // `from_core`, not per `draw()` call (Phase 6) — only the buffer's
        // contents change frame to frame.
        self.core
            .queue
            .write_buffer(&self.lights_buffer, 0, bytemuck::bytes_of(lights));

        let mut ui_buffers: Vec<(wgpu::Buffer, wgpu::Buffer, u32)> = Vec::new();
        let mut ui_uniform_state: Vec<(wgpu::Buffer, wgpu::BindGroup)> = Vec::new();

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("weft-render-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            for drawable in drawables {
                let uniforms = Uniforms {
                    view_proj: view_proj.to_cols_array_2d(),
                    model: drawable.transform.to_matrix().to_cols_array_2d(),
                    color: [drawable.color[0], drawable.color[1], drawable.color[2], 1.0],
                    material: [
                        drawable.roughness,
                        drawable.metallic,
                        drawable.normal_scale,
                        0.0,
                    ],
                    camera_pos: [camera_pos.x, camera_pos.y, camera_pos.z, 0.0],
                };
                let bind_group = write_pooled_buffer(
                    &self.core.device,
                    &self.core.queue,
                    &self.bind_group_layout,
                    &mut self.uniform_pool,
                    drawable.entity,
                    "weft-draw-uniforms",
                    bytemuck::bytes_of(&uniforms),
                    wgpu::BufferUsages::UNIFORM,
                );

                let texture_bind_group = match (
                    &drawable.texture,
                    &drawable.metallic_roughness_texture,
                    &drawable.normal_texture,
                ) {
                    (None, None, None) => &self.white_bind_group,
                    (base_color, metallic_roughness, normal) => {
                        let key = texture_cache_key(base_color, metallic_roughness, normal);
                        if !self.texture_cache.contains(&key) {
                            let bind_group = load_texture_bind_group(
                                base_color.as_deref(),
                                metallic_roughness.as_deref(),
                                normal.as_deref(),
                                &self.core.device,
                                &self.core.queue,
                                &asset_store,
                                &self.texture_bind_group_layout,
                                &self.sampler,
                                &self.white_view,
                                &self.flat_normal_view,
                            )?;
                            self.texture_cache.put(key.clone(), bind_group);
                        }
                        self.texture_cache.get(&key).unwrap()
                    }
                };

                // A skinned draw needs both a `skin` hash (per-vertex
                // joint/weight data) and a `JointPalette` on the same
                // entity (the computed matrices `animation_step` writes).
                // Missing either — e.g. `MeshRef.skin` set with no
                // `Animator`/`"animation"` system wired up — falls back to
                // an ordinary static draw in bind pose rather than
                // guessing a joint count or hard-failing; see `Drawable`'s
                // doc comment.
                let skinned = match (&drawable.mesh, &drawable.skin, &drawable.joint_matrices) {
                    (MeshKind::Asset(mesh_hash), Some(skin_hash), Some(matrices)) => Some((
                        mesh_hash.clone(),
                        skin_hash.clone(),
                        drawable.tangent.clone(),
                        matrices,
                    )),
                    _ => None,
                };

                if let Some((mesh_hash, skin_hash, tangent_hash, matrices)) = skinned {
                    let key = (mesh_hash, skin_hash, tangent_hash);
                    if !self.skin_cache.contains(&key) {
                        let buffers = load_skinned_mesh_buffers(
                            &key.0,
                            &key.1,
                            key.2.as_deref(),
                            &self.core.device,
                            &asset_store,
                        )?;
                        self.skin_cache.put(key.clone(), buffers);
                    }
                    let (vertex_buffer, index_buffer, index_count) =
                        self.skin_cache.get(&key).unwrap();

                    // Shared with the shadow pass above (Phase 6) — see
                    // that call site's comment.
                    let joint_bind_group = write_pooled_buffer(
                        &self.core.device,
                        &self.core.queue,
                        &self.joint_bind_group_layout,
                        &mut self.joint_pool,
                        drawable.entity,
                        "weft-joint-matrices",
                        bytemuck::cast_slice(matrices.as_slice()),
                        wgpu::BufferUsages::STORAGE,
                    );

                    pass.set_pipeline(&skinned_pipeline);
                    pass.set_bind_group(0, bind_group, &[]);
                    pass.set_bind_group(1, texture_bind_group, &[]);
                    pass.set_bind_group(2, joint_bind_group, &[]);
                    pass.set_bind_group(3, &self.lights_bind_group, &[]);
                    pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                    pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..*index_count, 0, 0..1);
                    continue;
                }

                let (vertex_buffer, index_buffer, index_count) = match &drawable.mesh {
                    MeshKind::Cube => (
                        &self.cube_buffers.0,
                        &self.cube_buffers.1,
                        self.cube_buffers.2,
                    ),
                    MeshKind::Plane => (
                        &self.plane_buffers.0,
                        &self.plane_buffers.1,
                        self.plane_buffers.2,
                    ),
                    MeshKind::Sphere => (
                        &self.sphere_buffers.0,
                        &self.sphere_buffers.1,
                        self.sphere_buffers.2,
                    ),
                    MeshKind::Asset(hash) => {
                        let key = (hash.clone(), drawable.tangent.clone());
                        if !self.mesh_cache.contains(&key) {
                            let buffers = load_mesh_buffers(
                                hash,
                                drawable.tangent.as_deref(),
                                &self.core.device,
                                &asset_store,
                            )?;
                            self.mesh_cache.put(key.clone(), buffers);
                        }
                        let (vb, ib, count) = self.mesh_cache.get(&key).unwrap();
                        (vb, ib, *count)
                    }
                };

                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, bind_group, &[]);
                pass.set_bind_group(1, texture_bind_group, &[]);
                pass.set_bind_group(2, &self.lights_bind_group, &[]);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..index_count, 0, 0..1);
            }

            if !texts.is_empty() {
                let screen_uniform = ScreenUniform {
                    size: [width as f32, height as f32],
                    _pad: [0.0, 0.0],
                };
                let screen_buffer =
                    self.core
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("weft-ui-screen-uniform"),
                            contents: bytemuck::bytes_of(&screen_uniform),
                            usage: wgpu::BufferUsages::UNIFORM,
                        });
                let screen_bind_group =
                    self.core
                        .device
                        .create_bind_group(&wgpu::BindGroupDescriptor {
                            label: Some("weft-ui-screen-bind-group"),
                            layout: &self.ui_bind_group_layout,
                            entries: &[wgpu::BindGroupEntry {
                                binding: 0,
                                resource: screen_buffer.as_entire_binding(),
                            }],
                        });
                ui_uniform_state.push((screen_buffer, screen_bind_group));
                let (_, screen_bind_group) = ui_uniform_state.last().unwrap();

                pass.set_pipeline(&ui_pipeline);
                pass.set_bind_group(0, screen_bind_group, &[]);

                // Grouped by font (a `BTreeMap` for stable iteration order,
                // per ADR-0002) so a HUD using two fonts costs two draw
                // calls, not one per `Text` entity.
                let mut groups: std::collections::BTreeMap<Option<&str>, Vec<&TextDrawable>> =
                    std::collections::BTreeMap::new();
                for t in texts {
                    groups.entry(t.font.as_deref()).or_default().push(t);
                }

                for (font_key, group) in &groups {
                    if let Some(hash) = font_key {
                        if !self.font_cache.contains(*hash) {
                            let bytes = asset_store.get(hash)?;
                            let atlas = GlyphAtlas::build(
                                &self.core.device,
                                &self.core.queue,
                                &self.texture_bind_group_layout,
                                &self.sampler,
                                &bytes,
                            )?;
                            self.font_cache.put((*hash).to_string(), atlas);
                        }
                    }
                    let atlas = match font_key {
                        None => &self.default_atlas,
                        Some(hash) => self.font_cache.get(*hash).unwrap(),
                    };

                    let mut vertices = Vec::new();
                    let mut indices = Vec::new();
                    for t in group {
                        text::layout(
                            atlas,
                            &t.content,
                            t.x,
                            t.y,
                            t.size,
                            t.color,
                            &mut vertices,
                            &mut indices,
                        );
                    }
                    if indices.is_empty() {
                        continue;
                    }

                    let vbuf =
                        self.core
                            .device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("weft-ui-vertices"),
                                contents: bytemuck::cast_slice(&vertices),
                                usage: wgpu::BufferUsages::VERTEX,
                            });
                    let ibuf =
                        self.core
                            .device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("weft-ui-indices"),
                                contents: bytemuck::cast_slice(&indices),
                                usage: wgpu::BufferUsages::INDEX,
                            });
                    ui_buffers.push((vbuf, ibuf, indices.len() as u32));
                    let (vbuf, ibuf, index_count) = ui_buffers.last().unwrap();

                    pass.set_bind_group(1, &atlas.bind_group, &[]);
                    pass.set_vertex_buffer(0, vbuf.slice(..));
                    pass.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..*index_count, 0, 0..1);
                }
            }
        }

        // Evict pool entries for entities no longer in this frame's
        // drawable set (Phase 6) — an entity despawned since the last frame
        // shouldn't keep its GPU buffer alive forever. The shadow-uniform
        // pool is only touched (created, written, *or* evicted) on frames
        // that actually ran the shadow pass — when the scene has no shadow
        // caster this frame, its entries are left alone rather than wiped,
        // since "no caster right now" says nothing about whether one will
        // reappear next frame.
        let live_entities: std::collections::HashSet<hecs::Entity> =
            drawables.iter().map(|d| d.entity).collect();
        self.uniform_pool.retain(|e, _| live_entities.contains(e));
        if lights.shadow_caster_index >= 0 {
            self.shadow_uniform_pool
                .retain(|e, _| live_entities.contains(e));
        }
        let live_skinned: std::collections::HashSet<hecs::Entity> = drawables
            .iter()
            .filter(|d| {
                matches!(
                    (&d.mesh, &d.skin, &d.joint_matrices),
                    (MeshKind::Asset(_), Some(_), Some(_))
                )
            })
            .map(|d| d.entity)
            .collect();
        self.joint_pool.retain(|e, _| live_skinned.contains(e));

        Ok(())
    }
}

fn make_depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("weft-depth-target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    depth_texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// `(view_proj, camera_pos, drawables, texts, lights)` — factored into a
/// named alias purely to keep clippy's `type_complexity` lint quiet; see
/// `extract_scene`'s own callers for how each element is used.
type ExtractedScene = (Mat4, Vec3, Vec<Drawable>, Vec<TextDrawable>, Lights);

/// The shadow-casting directional light's light-space view-projection
/// matrix: an orthographic frustum of fixed half-extent, centered on the
/// main camera's look-at `target` — not scene-bounds-fitted or cascaded, a
/// deliberate, named scoping limit (see Phase 5 / `visual-realism-plan.md`).
fn shadow_view_proj_for(direction: Vec3, target: Vec3) -> Mat4 {
    let dir = direction.normalize();
    // A look-at view needs an up vector not parallel to `dir` — `Y` fails
    // for a near-straight-down/up light, so fall back to `X`.
    let up = if dir.dot(Vec3::Y).abs() > 0.99 {
        Vec3::X
    } else {
        Vec3::Y
    };
    let eye = target - dir * (SHADOW_FAR * 0.5);
    let view = glam::camera::rh::view::look_at_mat4(eye, target, up);
    let proj = glam::camera::rh::proj::directx::orthographic(
        -SHADOW_ORTHO_HALF_EXTENT,
        SHADOW_ORTHO_HALF_EXTENT,
        -SHADOW_ORTHO_HALF_EXTENT,
        SHADOW_ORTHO_HALF_EXTENT,
        SHADOW_NEAR,
        SHADOW_FAR,
    );
    proj * view
}

fn extract_scene(
    world: &hecs::World,
    width: u32,
    height: u32,
) -> Result<ExtractedScene, RenderError> {
    let mut cameras: Vec<_> = world
        .query::<(&Transform, &Camera)>()
        .iter()
        .map(|(e, (t, c))| (e, *t, *c))
        .collect();
    cameras.sort_by_key(|(e, _, _)| e.to_bits());
    let (_, camera_transform, camera) = match cameras.len() {
        0 => return Err(RenderError::NoCamera),
        1 => cameras[0],
        n => return Err(RenderError::MultipleCameras(n)),
    };

    let mut drawables: Vec<_> = world
        .query::<(&Transform, &MeshRef, &Material, Option<&JointPalette>)>()
        .iter()
        .map(|(e, (t, m, mat, palette))| {
            (
                e,
                Drawable {
                    entity: e,
                    transform: *t,
                    mesh: m.mesh.clone(),
                    color: mat.color,
                    roughness: mat.roughness,
                    metallic: mat.metallic,
                    texture: mat.texture.clone(),
                    metallic_roughness_texture: mat.metallic_roughness_texture.clone(),
                    normal_texture: mat.normal_texture.clone(),
                    normal_scale: mat.normal_scale,
                    skin: m.skin.clone(),
                    joint_matrices: palette.map(|p| p.matrices.clone()),
                    tangent: m.tangent.clone(),
                },
            )
        })
        .collect();
    drawables.sort_by_key(|(e, _)| e.to_bits());

    let mut texts: Vec<_> = world
        .query::<&Text>()
        .iter()
        .map(|(e, t)| {
            (
                e,
                TextDrawable {
                    content: t.content.clone(),
                    x: t.x,
                    y: t.y,
                    size: t.size,
                    color: [t.color[0], t.color[1], t.color[2], 1.0],
                    font: t.font.clone(),
                },
            )
        })
        .collect();
    texts.sort_by_key(|(e, _)| e.to_bits());

    let mut scene_lights: Vec<_> = world
        .query::<(&Transform, &Light)>()
        .iter()
        .map(|(e, (t, l))| (e, *t, *l))
        .collect();
    scene_lights.sort_by_key(|(e, _, _)| e.to_bits());
    if scene_lights.len() > MAX_LIGHTS {
        return Err(RenderError::TooManyLights(scene_lights.len()));
    }

    // Shadow caster validation (Phase 5) — at most one `Light` may set
    // `casts_shadow`, and it must be `Directional` (point-light shadows are
    // out of scope, see `Light`'s doc comment).
    let shadow_caster_count = scene_lights
        .iter()
        .filter(|(_, _, l)| l.casts_shadow)
        .count();
    if shadow_caster_count > 1 {
        return Err(RenderError::MultipleShadowCasters(shadow_caster_count));
    }
    if let Some((_, _, light)) = scene_lights.iter().find(|(_, _, l)| l.casts_shadow) {
        if !matches!(light.kind, LightKind::Directional { .. }) {
            return Err(RenderError::UnsupportedShadowCaster);
        }
    }

    let mut gpu_lights = [GpuLight {
        pos_or_dir: [0.0; 4],
        color_intensity: [0.0; 4],
        range: [0.0; 4],
    }; MAX_LIGHTS];
    // Index into `gpu_lights` of the shadow caster, or `-1` if none — the
    // synthesized zero-light fallback below never casts a shadow.
    let mut shadow_caster_index: i32 = -1;
    let mut shadow_view_proj = Mat4::IDENTITY.to_cols_array_2d();
    let light_count = if scene_lights.is_empty() {
        // No `Light` entity anywhere in the scene — synthesize one fallback
        // directional light matching the pre-Phase-4 hardcoded look exactly,
        // so no existing scene file needs a `Light` added to keep rendering.
        gpu_lights[0] = GpuLight {
            pos_or_dir: [LIGHT_DIR.x, LIGHT_DIR.y, LIGHT_DIR.z, 0.0],
            color_intensity: [1.0, 1.0, 1.0, 0.85],
            range: [0.0; 4],
        };
        1
    } else {
        let count = scene_lights.len();
        for (i, (_, transform, light)) in scene_lights.into_iter().enumerate() {
            gpu_lights[i] = match light.kind {
                LightKind::Directional { direction } => GpuLight {
                    pos_or_dir: [direction.x, direction.y, direction.z, 0.0],
                    color_intensity: [
                        light.color[0],
                        light.color[1],
                        light.color[2],
                        light.intensity,
                    ],
                    range: [0.0; 4],
                },
                LightKind::Point { range } => GpuLight {
                    pos_or_dir: [
                        transform.position.x,
                        transform.position.y,
                        transform.position.z,
                        1.0,
                    ],
                    color_intensity: [
                        light.color[0],
                        light.color[1],
                        light.color[2],
                        light.intensity,
                    ],
                    range: [range, 0.0, 0.0, 0.0],
                },
            };
            if light.casts_shadow {
                shadow_caster_index = i as i32;
                if let LightKind::Directional { direction } = light.kind {
                    shadow_view_proj =
                        shadow_view_proj_for(direction, camera.target).to_cols_array_2d();
                }
            }
        }
        count
    };
    let lights = Lights {
        lights: gpu_lights,
        shadow_view_proj,
        count: light_count as u32,
        shadow_caster_index,
        _pad: [0; 2],
    };

    let aspect = width as f32 / height as f32;
    let projection = glam::camera::rh::proj::directx::perspective(
        camera.fov_y_degrees.to_radians(),
        aspect,
        camera.near,
        camera.far,
    );
    let view =
        glam::camera::rh::view::look_at_mat4(camera_transform.position, camera.target, Vec3::Y);
    let view_proj = projection * view;

    Ok((
        view_proj,
        camera_transform.position,
        drawables.into_iter().map(|(_, d)| d).collect(),
        texts.into_iter().map(|(_, t)| t).collect(),
        lights,
    ))
}

/// Renders `world` at `width`x`height` and returns the pixels. Builds a
/// fresh, one-shot `RenderContext` every call — the right tradeoff for a
/// single PNG export, but not for a real-time loop (see
/// `render_scene_with_context` / `WindowRenderer` for the persisted-context
/// path a live loop should use instead).
pub fn render_scene(
    world: &hecs::World,
    width: u32,
    height: u32,
    assets_dir: &Path,
) -> Result<image::RgbaImage, RenderError> {
    let mut ctx = RenderContext::new_headless(wgpu::Backends::VULKAN)?;
    render_scene_with_context(&mut ctx, world, width, height, assets_dir)
}

/// Like `render_scene`, but reuses an existing `RenderContext` (device,
/// pipelines, mesh/texture caches) instead of building a new one — the path
/// a caller rendering many frames (a live loop, a batch of scenes) should
/// use. Not itself called by `WindowRenderer` — windowed presentation has
/// no CPU pixel readback at all (see `WindowRenderer::render`), which is
/// the actual perf-relevant difference; this fn's `read_back` stall is fine
/// once per `engine render` invocation, not fine per frame.
pub fn render_scene_with_context(
    ctx: &mut RenderContext,
    world: &hecs::World,
    width: u32,
    height: u32,
    assets_dir: &Path,
) -> Result<image::RgbaImage, RenderError> {
    let (view_proj, camera_pos, drawables, texts, lights) = extract_scene(world, width, height)?;

    let color_texture = ctx.core.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("weft-color-target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: COLOR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = make_depth_view(&ctx.core.device, width, height);

    let mut encoder = ctx
        .core
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("weft-render-encoder"),
        });
    ctx.draw(
        &mut encoder,
        &drawables,
        &texts,
        view_proj,
        camera_pos,
        &lights,
        &color_view,
        COLOR_FORMAT,
        &depth_view,
        width,
        height,
        assets_dir,
    )?;

    read_back(
        &ctx.core.device,
        &ctx.core.queue,
        encoder,
        &color_texture,
        width,
        height,
    )
}

pub(crate) fn draw_to_surface(
    ctx: &mut RenderContext,
    world: &hecs::World,
    color_view: &wgpu::TextureView,
    color_format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    assets_dir: &Path,
) -> Result<wgpu::CommandBuffer, RenderError> {
    let (view_proj, camera_pos, drawables, texts, lights) = extract_scene(world, width, height)?;
    let depth_view = make_depth_view(&ctx.core.device, width, height);
    let mut encoder = ctx
        .core
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("weft-window-render-encoder"),
        });
    ctx.draw(
        &mut encoder,
        &drawables,
        &texts,
        view_proj,
        camera_pos,
        &lights,
        color_view,
        color_format,
        &depth_view,
        width,
        height,
        assets_dir,
    )?;
    Ok(encoder.finish())
}

fn upload_mesh(device: &wgpu::Device, data: &mesh::MeshData) -> MeshBuffers {
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("weft-mesh-vertices"),
        contents: bytemuck::cast_slice(&data.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("weft-mesh-indices"),
        contents: bytemuck::cast_slice(&data.indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    (vertex_buffer, index_buffer, data.indices.len() as u32)
}

fn load_mesh_buffers(
    hash: &str,
    tangent_hash: Option<&str>,
    device: &wgpu::Device,
    store: &engine_assets::AssetStore,
) -> Result<MeshBuffers, RenderError> {
    let bytes = store.get(hash)?;
    let mesh_data = engine_assets::mesh::decode(&bytes)?;
    let tangent_data = match tangent_hash {
        Some(h) => Some(engine_assets::tangent::decode(&store.get(h)?)?),
        None => None,
    };
    let render_mesh = mesh::from_asset(&mesh_data, tangent_data.as_ref())?;
    Ok(upload_mesh(device, &render_mesh))
}

fn upload_skinned_mesh(device: &wgpu::Device, data: &mesh::SkinnedMeshData) -> MeshBuffers {
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("weft-skinned-mesh-vertices"),
        contents: bytemuck::cast_slice(&data.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("weft-skinned-mesh-indices"),
        contents: bytemuck::cast_slice(&data.indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    (vertex_buffer, index_buffer, data.indices.len() as u32)
}

fn load_skinned_mesh_buffers(
    mesh_hash: &str,
    skin_hash: &str,
    tangent_hash: Option<&str>,
    device: &wgpu::Device,
    store: &engine_assets::AssetStore,
) -> Result<MeshBuffers, RenderError> {
    let mesh_bytes = store.get(mesh_hash)?;
    let mesh_data = engine_assets::mesh::decode(&mesh_bytes)?;
    let skin_bytes = store.get(skin_hash)?;
    let skin_data = engine_assets::skin::decode(&skin_bytes)?;
    let tangent_data = match tangent_hash {
        Some(h) => Some(engine_assets::tangent::decode(&store.get(h)?)?),
        None => None,
    };
    let skinned_mesh = mesh::from_skinned_asset(&mesh_data, &skin_data, tangent_data.as_ref())?;
    Ok(upload_skinned_mesh(device, &skinned_mesh))
}

/// The `texture_cache` key for a drawable's texture triple — `unwrap_or_default`
/// on the base-color slot is safe here since this is only ever called when
/// at least one of the three is `Some` (the `(None, None, None)` case
/// bypasses the cache entirely, using `white_bind_group` directly).
fn texture_cache_key(
    base_color: &Option<String>,
    metallic_roughness: &Option<String>,
    normal: &Option<String>,
) -> (String, Option<String>, Option<String>) {
    (
        base_color.clone().unwrap_or_default(),
        metallic_roughness.clone(),
        normal.clone(),
    )
}

/// `format` matters: base-color textures are authored/stored as sRGB (so
/// sampling must gamma-decode them), but the metallic-roughness texture's
/// G/B channels are raw linear scalars per the glTF spec — sampling those
/// through an sRGB view would silently skew every roughness/metallic value.
/// Both cases share this one upload path via the `format` parameter rather
/// than duplicating it, since the write-texture/view-creation logic is
/// otherwise identical.
fn upload_rgba_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    rgba: &image::RgbaImage,
    format: wgpu::TextureFormat,
) -> wgpu::TextureView {
    let (width, height) = rgba.dimensions();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("weft-texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// A 1x1 white view — simultaneously a no-op color tint and a no-op
/// rough/metal multiplier, so it serves as the shared default for every
/// slot in `texture_bind_group_layout`. Uploaded once and retained on
/// `RenderContext` (see its `white_view` field) rather than rebuilt per
/// bind group.
fn upload_white_texture_view(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let white = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
    upload_rgba_texture(device, queue, &white, wgpu::TextureFormat::Rgba8UnormSrgb)
}

/// A 1x1 flat tangent-space-normal view `(128,128,255)` — decodes to
/// `(0,0,1)`, the "no perturbation" default for the normal-map slot (Phase
/// 3). Linear, not sRGB — its channels are packed directions, not color,
/// same reasoning as the metallic-roughness texture's `load_rgba_view`
/// call below.
fn upload_flat_normal_view(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let flat = image::RgbaImage::from_pixel(1, 1, image::Rgba([128, 128, 255, 255]));
    upload_rgba_texture(device, queue, &flat, wgpu::TextureFormat::Rgba8Unorm)
}

fn create_white_texture_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    white_view: &wgpu::TextureView,
    flat_normal_view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("weft-white-texture-bind-group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(white_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(white_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(flat_normal_view),
            },
        ],
    })
}

/// Loads and uploads one imported texture asset by content hash. `srgb`
/// selects the color space (`true` for base color, `false` for the
/// metallic-roughness texture's linear channel data — see
/// `upload_rgba_texture`'s doc comment).
fn load_rgba_view(
    hash: &str,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    store: &engine_assets::AssetStore,
    srgb: bool,
) -> Result<wgpu::TextureView, RenderError> {
    let bytes = store.get(hash)?;
    let decoded = image::load_from_memory(&bytes).map_err(|source| {
        RenderError::AssetLoadFailed(engine_assets::AssetError::ImageDecodeFailed {
            path: hash.to_string(),
            source,
        })
    })?;
    let format = if srgb {
        wgpu::TextureFormat::Rgba8UnormSrgb
    } else {
        wgpu::TextureFormat::Rgba8Unorm
    };
    Ok(upload_rgba_texture(
        device,
        queue,
        &decoded.to_rgba8(),
        format,
    ))
}

#[allow(clippy::too_many_arguments)]
fn load_texture_bind_group(
    base_color_hash: Option<&str>,
    metallic_roughness_hash: Option<&str>,
    normal_hash: Option<&str>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    store: &engine_assets::AssetStore,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    white_view: &wgpu::TextureView,
    flat_normal_view: &wgpu::TextureView,
) -> Result<wgpu::BindGroup, RenderError> {
    let color_view = match base_color_hash {
        Some(hash) => load_rgba_view(hash, device, queue, store, true)?,
        None => white_view.clone(),
    };
    let mr_view = match metallic_roughness_hash {
        Some(hash) => load_rgba_view(hash, device, queue, store, false)?,
        None => white_view.clone(),
    };
    let normal_view = match normal_hash {
        Some(hash) => load_rgba_view(hash, device, queue, store, false)?,
        None => flat_normal_view.clone(),
    };
    Ok(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("weft-imported-texture-bind-group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&color_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&mr_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&normal_view),
            },
        ],
    }))
}

fn read_back(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mut encoder: wgpu::CommandEncoder,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Result<image::RgbaImage, RenderError> {
    let unpadded_bytes_per_row = width * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;

    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("weft-readback-buffer"),
        size: (padded_bytes_per_row * height) as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &output_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    queue.submit(std::iter::once(encoder.finish()));

    let slice = output_buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| RenderError::ReadbackFailed(e.to_string()))?;
    rx.recv()
        .map_err(|e| RenderError::ReadbackFailed(e.to_string()))?
        .map_err(|e| RenderError::ReadbackFailed(e.to_string()))?;

    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    {
        let view = slice
            .get_mapped_range()
            .map_err(|e| RenderError::ReadbackFailed(e.to_string()))?;
        for row in 0..height {
            let start = (row * padded_bytes_per_row) as usize;
            let end = start + unpadded_bytes_per_row as usize;
            pixels.extend_from_slice(&view[start..end]);
        }
    }
    output_buffer.unmap();

    image::RgbaImage::from_raw(width, height, pixels)
        .ok_or_else(|| RenderError::ReadbackFailed("pixel buffer size mismatch".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two materials sharing a base-color hash but differing
    /// metallic-roughness hashes must produce distinct `texture_cache`
    /// keys — otherwise the second material's draw would silently reuse
    /// the first's bind group and render with the wrong metallic-roughness
    /// texture (see Phase 2 / ADR-0019).
    #[test]
    fn texture_cache_key_distinguishes_by_metallic_roughness_hash_too() {
        let base_color = Some("base-color-hash".to_string());
        let key_a = texture_cache_key(&base_color, &Some("mr-hash-a".to_string()), &None);
        let key_b = texture_cache_key(&base_color, &Some("mr-hash-b".to_string()), &None);
        assert_ne!(key_a, key_b);
    }

    /// Same base-color and metallic-roughness hashes, differing only by
    /// normal-map hash — must still key distinctly, or a second material
    /// would silently render with the first's normal map.
    #[test]
    fn texture_cache_key_distinguishes_by_normal_hash_too() {
        let base_color = Some("base-color-hash".to_string());
        let key_a = texture_cache_key(&base_color, &None, &Some("normal-hash-a".to_string()));
        let key_b = texture_cache_key(&base_color, &None, &Some("normal-hash-b".to_string()));
        assert_ne!(key_a, key_b);
    }

    /// Same base-color hash, no metallic-roughness/normal texture on either
    /// side — must key identically so the cache actually hits.
    #[test]
    fn texture_cache_key_matches_when_both_hashes_match() {
        let base_color = Some("base-color-hash".to_string());
        let key_a = texture_cache_key(&base_color, &None, &None);
        let key_b = texture_cache_key(&base_color, &None, &None);
        assert_eq!(key_a, key_b);
    }

    fn camera_at(position: Vec3) -> (Transform, Camera) {
        (
            Transform::from_position(position),
            Camera {
                target: Vec3::ZERO,
                fov_y_degrees: 45.0,
                near: 0.1,
                far: 100.0,
            },
        )
    }

    /// A scene with zero `Light` entities must keep rendering under the
    /// exact pre-Phase-4 hardcoded look, not go dark — see `extract_scene`'s
    /// own doc comment.
    #[test]
    fn zero_light_scene_falls_back_to_the_pre_phase_4_hardcoded_light() {
        let mut world = hecs::World::new();
        world.spawn(camera_at(Vec3::new(0.0, 0.0, 5.0)));

        let (_, _, _, _, lights) = extract_scene(&world, 16, 16).unwrap();
        assert_eq!(lights.count, 1);
        let light = lights.lights[0];
        assert_eq!(
            light.pos_or_dir,
            [LIGHT_DIR.x, LIGHT_DIR.y, LIGHT_DIR.z, 0.0]
        );
        assert_eq!(light.color_intensity, [1.0, 1.0, 1.0, 0.85]);
    }

    /// More than `MAX_LIGHTS` `Light` entities in one scene is a structured
    /// error, not silent truncation or a panic.
    #[test]
    fn more_than_max_lights_is_a_structured_error() {
        let mut world = hecs::World::new();
        world.spawn(camera_at(Vec3::new(0.0, 0.0, 5.0)));
        for _ in 0..(MAX_LIGHTS + 1) {
            world.spawn((
                Transform::default(),
                Light {
                    kind: LightKind::Point { range: 5.0 },
                    color: [1.0, 1.0, 1.0],
                    intensity: 1.0,
                    casts_shadow: false,
                },
            ));
        }

        let err = match extract_scene(&world, 16, 16) {
            Err(e) => e,
            Ok(_) => panic!("expected RenderError::TooManyLights"),
        };
        assert_eq!(err.code(), "RENDER_TOO_MANY_LIGHTS");
    }

    /// Lights must be collected in deterministic entity-id order (per
    /// ADR-0002), matching cameras/drawables/texts' existing convention —
    /// not hecs' unspecified query iteration order.
    #[test]
    fn lights_are_collected_in_deterministic_entity_order() {
        let mut world = hecs::World::new();
        world.spawn(camera_at(Vec3::new(0.0, 0.0, 5.0)));
        world.spawn((
            Transform::default(),
            Light {
                kind: LightKind::Point { range: 1.0 },
                color: [1.0, 0.0, 0.0],
                intensity: 1.0,
                casts_shadow: false,
            },
        ));
        world.spawn((
            Transform::default(),
            Light {
                kind: LightKind::Point { range: 2.0 },
                color: [0.0, 1.0, 0.0],
                intensity: 2.0,
                casts_shadow: false,
            },
        ));

        let (_, _, _, _, lights) = extract_scene(&world, 16, 16).unwrap();
        assert_eq!(lights.count, 2);
        assert_eq!(lights.lights[0].color_intensity, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(lights.lights[1].color_intensity, [0.0, 1.0, 0.0, 2.0]);
    }

    fn directional_light(casts_shadow: bool) -> Light {
        Light {
            kind: LightKind::Directional {
                direction: Vec3::new(-0.2, -1.0, -0.1),
            },
            color: [1.0, 1.0, 1.0],
            intensity: 1.0,
            casts_shadow,
        }
    }

    /// More than one `casts_shadow: true` `Light` in one scene is a
    /// structured error, not "last one wins" or a silent pick.
    #[test]
    fn more_than_one_shadow_caster_is_a_structured_error() {
        let mut world = hecs::World::new();
        world.spawn(camera_at(Vec3::new(0.0, 0.0, 5.0)));
        world.spawn((Transform::default(), directional_light(true)));
        world.spawn((Transform::default(), directional_light(true)));

        let err = match extract_scene(&world, 16, 16) {
            Err(e) => e,
            Ok(_) => panic!("expected RenderError::MultipleShadowCasters"),
        };
        assert_eq!(err.code(), "RENDER_MULTIPLE_SHADOW_CASTERS");
    }

    /// A `casts_shadow: true` `Point` light is a structured error — point-
    /// light shadows aren't supported (see `Light`'s doc comment).
    #[test]
    fn a_point_light_shadow_caster_is_a_structured_error() {
        let mut world = hecs::World::new();
        world.spawn(camera_at(Vec3::new(0.0, 0.0, 5.0)));
        world.spawn((
            Transform::default(),
            Light {
                kind: LightKind::Point { range: 5.0 },
                color: [1.0, 1.0, 1.0],
                intensity: 1.0,
                casts_shadow: true,
            },
        ));

        let err = match extract_scene(&world, 16, 16) {
            Err(e) => e,
            Ok(_) => panic!("expected RenderError::UnsupportedShadowCaster"),
        };
        assert_eq!(err.code(), "RENDER_UNSUPPORTED_SHADOW_CASTER");
    }

    /// A scene with no `casts_shadow: true` light (including the zero-light
    /// fallback case) must leave `shadow_caster_index` at the "none" sentinel,
    /// so the shader's shadow branch never runs.
    #[test]
    fn zero_shadow_casters_leaves_shadow_caster_index_at_minus_one() {
        let mut world = hecs::World::new();
        world.spawn(camera_at(Vec3::new(0.0, 0.0, 5.0)));
        world.spawn((Transform::default(), directional_light(false)));

        let (_, _, _, _, lights) = extract_scene(&world, 16, 16).unwrap();
        assert_eq!(lights.shadow_caster_index, -1);
    }
}

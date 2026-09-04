use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::Path;

use glam::{Mat4, Vec3};
use lru::LruCache;
use wgpu::util::DeviceExt;

use crate::components::{Camera, Material, MeshKind, MeshRef, Text};
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

/// A hardcoded key light — simple-lit, not PBR, per Phase 2's scope.
const LIGHT_DIR: Vec3 = Vec3::new(-0.4, -1.0, -0.3);

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    model: [[f32; 4]; 4],
    color: [f32; 4],
    light_dir: [f32; 4],
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
    transform: Transform,
    mesh: MeshKind,
    color: [f32; 3],
    roughness: f32,
    metallic: f32,
    texture: Option<String>,
    /// A content hash into the `SkinData` asset store, plus the same
    /// entity's `JointPalette` matrices (if any) — both must be present to
    /// actually draw skinned; see `draw()`'s `use_skinned` check for why a
    /// `skin` with no palette yet falls back to an ordinary static draw
    /// rather than erroring or guessing a joint count.
    skin: Option<String>,
    joint_matrices: Option<Vec<[[f32; 4]; 4]>>,
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
    white_bind_group: wgpu::BindGroup,
    cube_buffers: MeshBuffers,
    plane_buffers: MeshBuffers,
    sphere_buffers: MeshBuffers,
    mesh_cache: LruCache<String, MeshBuffers>,
    texture_cache: LruCache<String, wgpu::BindGroup>,
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
    skin_cache: LruCache<(String, String), MeshBuffers>,
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
        ],
    }
}

fn skinned_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const JOINTS_OFFSET: wgpu::BufferAddress = (std::mem::size_of::<[f32; 3]>() * 2
        + std::mem::size_of::<[f32; 2]>())
        as wgpu::BufferAddress;
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
                ],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("weft-pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout), Some(&texture_bind_group_layout)],
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
                ],
                immediate_size: 0,
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
        // look exactly, so one shader code path covers both cases.
        let white_bind_group = create_white_texture_bind_group(
            device,
            &core.queue,
            &texture_bind_group_layout,
            &sampler,
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

        Ok(Self {
            core,
            shader,
            bind_group_layout,
            texture_bind_group_layout,
            pipeline_layout,
            pipelines: HashMap::new(),
            sampler,
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
            ui_shader,
            ui_bind_group_layout,
            ui_pipeline_layout,
            ui_pipelines: HashMap::new(),
            default_atlas,
            font_cache: LruCache::new(CACHE_CAPACITY),
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

        let mut per_draw_state = Vec::with_capacity(drawables.len());
        let mut joint_buffers: Vec<(wgpu::Buffer, wgpu::BindGroup)> = Vec::new();
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
                    light_dir: [LIGHT_DIR.x, LIGHT_DIR.y, LIGHT_DIR.z, 0.0],
                    material: [drawable.roughness, drawable.metallic, 0.0, 0.0],
                    camera_pos: [camera_pos.x, camera_pos.y, camera_pos.z, 0.0],
                };
                let uniform_buffer =
                    self.core
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("weft-draw-uniforms"),
                            contents: bytemuck::bytes_of(&uniforms),
                            usage: wgpu::BufferUsages::UNIFORM,
                        });
                let bind_group = self
                    .core
                    .device
                    .create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("weft-draw-bind-group"),
                        layout: &self.bind_group_layout,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: uniform_buffer.as_entire_binding(),
                        }],
                    });
                per_draw_state.push((uniform_buffer, bind_group));

                let texture_bind_group = match &drawable.texture {
                    Some(hash) => {
                        if !self.texture_cache.contains(hash) {
                            let bind_group = load_texture_bind_group(
                                hash,
                                &self.core.device,
                                &self.core.queue,
                                &asset_store,
                                &self.texture_bind_group_layout,
                                &self.sampler,
                            )?;
                            self.texture_cache.put(hash.clone(), bind_group);
                        }
                        self.texture_cache.get(hash).unwrap()
                    }
                    None => &self.white_bind_group,
                };

                let (_, bind_group) = per_draw_state.last().unwrap();

                // A skinned draw needs both a `skin` hash (per-vertex
                // joint/weight data) and a `JointPalette` on the same
                // entity (the computed matrices `animation_step` writes).
                // Missing either — e.g. `MeshRef.skin` set with no
                // `Animator`/`"animation"` system wired up — falls back to
                // an ordinary static draw in bind pose rather than
                // guessing a joint count or hard-failing; see `Drawable`'s
                // doc comment.
                let skinned = match (&drawable.mesh, &drawable.skin, &drawable.joint_matrices) {
                    (MeshKind::Asset(mesh_hash), Some(skin_hash), Some(matrices)) => {
                        Some((mesh_hash.clone(), skin_hash.clone(), matrices))
                    }
                    _ => None,
                };

                if let Some((mesh_hash, skin_hash, matrices)) = skinned {
                    let key = (mesh_hash, skin_hash);
                    if !self.skin_cache.contains(&key) {
                        let buffers = load_skinned_mesh_buffers(
                            &key.0,
                            &key.1,
                            &self.core.device,
                            &asset_store,
                        )?;
                        self.skin_cache.put(key.clone(), buffers);
                    }
                    let (vertex_buffer, index_buffer, index_count) =
                        self.skin_cache.get(&key).unwrap();

                    let joint_buffer =
                        self.core
                            .device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("weft-joint-matrices"),
                                contents: bytemuck::cast_slice(matrices.as_slice()),
                                usage: wgpu::BufferUsages::STORAGE,
                            });
                    let joint_bind_group =
                        self.core
                            .device
                            .create_bind_group(&wgpu::BindGroupDescriptor {
                                label: Some("weft-joint-bind-group"),
                                layout: &self.joint_bind_group_layout,
                                entries: &[wgpu::BindGroupEntry {
                                    binding: 0,
                                    resource: joint_buffer.as_entire_binding(),
                                }],
                            });
                    joint_buffers.push((joint_buffer, joint_bind_group));
                    let (_, joint_bind_group) = joint_buffers.last().unwrap();

                    pass.set_pipeline(&skinned_pipeline);
                    pass.set_bind_group(0, bind_group, &[]);
                    pass.set_bind_group(1, texture_bind_group, &[]);
                    pass.set_bind_group(2, joint_bind_group, &[]);
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
                        if !self.mesh_cache.contains(hash) {
                            let buffers = load_mesh_buffers(hash, &self.core.device, &asset_store)?;
                            self.mesh_cache.put(hash.clone(), buffers);
                        }
                        let (vb, ib, count) = self.mesh_cache.get(hash).unwrap();
                        (vb, ib, *count)
                    }
                };

                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, bind_group, &[]);
                pass.set_bind_group(1, texture_bind_group, &[]);
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

fn extract_scene(
    world: &hecs::World,
    width: u32,
    height: u32,
) -> Result<(Mat4, Vec3, Vec<Drawable>, Vec<TextDrawable>), RenderError> {
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
                    transform: *t,
                    mesh: m.mesh.clone(),
                    color: mat.color,
                    roughness: mat.roughness,
                    metallic: mat.metallic,
                    texture: mat.texture.clone(),
                    skin: m.skin.clone(),
                    joint_matrices: palette.map(|p| p.matrices.clone()),
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
    let (view_proj, camera_pos, drawables, texts) = extract_scene(world, width, height)?;

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
    let (view_proj, camera_pos, drawables, texts) = extract_scene(world, width, height)?;
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
    device: &wgpu::Device,
    store: &engine_assets::AssetStore,
) -> Result<MeshBuffers, RenderError> {
    let bytes = store.get(hash)?;
    let mesh_data = engine_assets::mesh::decode(&bytes)?;
    let render_mesh = mesh::from_asset(&mesh_data);
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
    device: &wgpu::Device,
    store: &engine_assets::AssetStore,
) -> Result<MeshBuffers, RenderError> {
    let mesh_bytes = store.get(mesh_hash)?;
    let mesh_data = engine_assets::mesh::decode(&mesh_bytes)?;
    let skin_bytes = store.get(skin_hash)?;
    let skin_data = engine_assets::skin::decode(&skin_bytes)?;
    let skinned_mesh = mesh::from_skinned_asset(&mesh_data, &skin_data)?;
    Ok(upload_skinned_mesh(device, &skinned_mesh))
}

fn upload_rgba_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    rgba: &image::RgbaImage,
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
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
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

fn create_white_texture_bind_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    let white = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
    let view = upload_rgba_texture(device, queue, &white);
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("weft-white-texture-bind-group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn load_texture_bind_group(
    hash: &str,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    store: &engine_assets::AssetStore,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
) -> Result<wgpu::BindGroup, RenderError> {
    let bytes = store.get(hash)?;
    let decoded = image::load_from_memory(&bytes).map_err(|source| {
        RenderError::AssetLoadFailed(engine_assets::AssetError::ImageDecodeFailed {
            path: hash.to_string(),
            source,
        })
    })?;
    let view = upload_rgba_texture(device, queue, &decoded.to_rgba8());
    Ok(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("weft-imported-texture-bind-group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
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

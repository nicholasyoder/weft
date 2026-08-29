use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

use crate::components::{Camera, Material, MeshKind, MeshRef};
use crate::error::RenderError;
use crate::mesh::{self, Vertex};
use engine_core::Transform;

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// A hardcoded key light — simple-lit, not PBR, per Phase 2's scope.
const LIGHT_DIR: Vec3 = Vec3::new(-0.4, -1.0, -0.3);

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    model: [[f32; 4]; 4],
    color: [f32; 4],
    light_dir: [f32; 4],
}

struct Drawable {
    transform: Transform,
    mesh: MeshKind,
    color: [f32; 3],
}

pub fn render_scene(
    world: &hecs::World,
    width: u32,
    height: u32,
) -> Result<image::RgbaImage, RenderError> {
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
        .query::<(&Transform, &MeshRef, &Material)>()
        .iter()
        .map(|(e, (t, m, mat))| {
            (
                e,
                Drawable {
                    transform: *t,
                    mesh: m.mesh,
                    color: mat.color,
                },
            )
        })
        .collect();
    drawables.sort_by_key(|(e, _)| e.to_bits());

    let aspect = width as f32 / height as f32;
    let projection = Mat4::perspective_rh(
        camera.fov_y_degrees.to_radians(),
        aspect,
        camera.near,
        camera.far,
    );
    let view = Mat4::look_at_rh(camera_transform.position, camera.target, Vec3::Y);
    let view_proj = projection * view;

    pollster::block_on(render(
        &drawables.into_iter().map(|(_, d)| d).collect::<Vec<_>>(),
        view_proj,
        width,
        height,
    ))
}

async fn render(
    drawables: &[Drawable],
    view_proj: Mat4,
    width: u32,
    height: u32,
) -> Result<image::RgbaImage, RenderError> {
    let mut instance_desc = wgpu::InstanceDescriptor::new_without_display_handle();
    instance_desc.backends = wgpu::Backends::VULKAN;
    let instance = wgpu::Instance::new(instance_desc);

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::None,
            compatible_surface: None,
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

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("weft-pipeline-layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });

    let vertex_layout = wgpu::VertexBufferLayout {
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
        ],
    };

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("weft-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[Some(vertex_layout)],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: COLOR_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
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
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    let cube_mesh = mesh::cube();
    let plane_mesh = mesh::plane();
    let cube_buffers = upload_mesh(&device, &cube_mesh);
    let plane_buffers = upload_mesh(&device, &plane_mesh);

    let color_texture = device.create_texture(&wgpu::TextureDescriptor {
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
    let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("weft-render-encoder"),
    });

    // One uniform buffer + bind group per draw call: entity counts here are
    // tiny (Phase 2 hardcoded-mesh scenes), so per-draw allocation is far
    // simpler than dynamic-offset bookkeeping and easy to revisit if
    // profiling ever says otherwise.
    let mut per_draw_state = Vec::with_capacity(drawables.len());

    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("weft-render-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
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
                view: &depth_view,
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

        pass.set_pipeline(&pipeline);

        for drawable in drawables {
            let uniforms = Uniforms {
                view_proj: view_proj.to_cols_array_2d(),
                model: drawable.transform.to_matrix().to_cols_array_2d(),
                color: [drawable.color[0], drawable.color[1], drawable.color[2], 1.0],
                light_dir: [LIGHT_DIR.x, LIGHT_DIR.y, LIGHT_DIR.z, 0.0],
            };
            let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("weft-draw-uniforms"),
                contents: bytemuck::bytes_of(&uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("weft-draw-bind-group"),
                layout: &bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                }],
            });
            per_draw_state.push((uniform_buffer, bind_group));
            let (vertex_buffer, index_buffer, index_count) = match drawable.mesh {
                MeshKind::Cube => (&cube_buffers.0, &cube_buffers.1, cube_buffers.2),
                MeshKind::Plane => (&plane_buffers.0, &plane_buffers.1, plane_buffers.2),
            };
            let (_, bind_group) = per_draw_state.last().unwrap();
            pass.set_bind_group(0, bind_group, &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..index_count, 0, 0..1);
        }
    }

    let image = read_back(&device, &queue, encoder, &color_texture, width, height)?;
    Ok(image)
}

fn upload_mesh(device: &wgpu::Device, data: &mesh::MeshData) -> (wgpu::Buffer, wgpu::Buffer, u32) {
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

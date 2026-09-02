//! Glyph rasterization/atlas-packing/layout for the screen-space UI text
//! pass — kept hand-rolled on top of `fontdue` (which only rasterizes,
//! nothing more) rather than pulling in a full text-rendering framework
//! like `glyphon`, so the atlas/pipeline/draw-call integration stays in
//! this crate's existing style. See ADR-0014.

use std::collections::HashMap;

use fontdue::{Font, FontSettings};

use crate::error::RenderError;

/// The pixel size glyphs are rasterized at when building an atlas.
/// `Text.size` then scales the resulting quads geometrically at layout
/// time — no SDF, so quality softens the further a given `Text.size`
/// drifts from this base (an SDF atlas is a natural Tier 3 polish
/// upgrade, not built here).
const BASE_PX: f32 = 48.0;
const FIRST_CHAR: u32 = 0x20; // ' '
const LAST_CHAR: u32 = 0x7E; // '~' — printable ASCII only in v1.
const ATLAS_WIDTH: u32 = 512;
const ATLAS_PADDING: u32 = 1;

/// The engine's embedded fallback font, used whenever a `Text` entity
/// doesn't name a custom font — see the `Text::font` doc comment. OFL
/// licensed; license text alongside it in `assets/fonts/`.
pub(crate) const DEFAULT_FONT_BYTES: &[u8] =
    include_bytes!("../assets/fonts/Inconsolata-Regular.ttf");

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct UiVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

pub(crate) fn ui_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<UiVertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                offset: (std::mem::size_of::<[f32; 2]>() * 2) as wgpu::BufferAddress,
                shader_location: 2,
                format: wgpu::VertexFormat::Float32x4,
            },
        ],
    }
}

struct GlyphInfo {
    uv_min: [f32; 2],
    uv_max: [f32; 2],
    width: f32,
    height: f32,
    xmin: f32,
    ymin: f32,
    advance: f32,
}

/// A font rasterized once into a single-channel coverage-bitmap atlas
/// covering printable ASCII, cached by content hash in
/// `RenderContext::font_cache` exactly like `texture_cache` already caches
/// imported images — built lazily on first use of a `Text.font` hash, not
/// eagerly for every font a scene might ever reference.
pub(crate) struct GlyphAtlas {
    pub bind_group: wgpu::BindGroup,
    glyphs: HashMap<char, GlyphInfo>,
    ascent: f32,
    line_height: f32,
}

impl GlyphAtlas {
    pub fn build(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture_bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        font_bytes: &[u8],
    ) -> Result<Self, RenderError> {
        let font = Font::from_bytes(font_bytes, FontSettings::default())
            .map_err(|e| RenderError::FontParseFailed(e.to_string()))?;

        struct Raster {
            ch: char,
            metrics: fontdue::Metrics,
            bitmap: Vec<u8>,
        }
        let rasters: Vec<Raster> = (FIRST_CHAR..=LAST_CHAR)
            .filter_map(char::from_u32)
            .map(|ch| {
                let (metrics, bitmap) = font.rasterize(ch, BASE_PX);
                Raster {
                    ch,
                    metrics,
                    bitmap,
                }
            })
            .collect();

        // Simple shelf packing — printable ASCII at one size is ~95 small
        // bitmaps, nowhere near enough to justify a real bin-packer.
        let mut cursor_x = ATLAS_PADDING;
        let mut cursor_y = ATLAS_PADDING;
        let mut row_height = 0u32;
        let mut placements = Vec::with_capacity(rasters.len());
        for r in &rasters {
            let w = r.metrics.width as u32;
            let h = r.metrics.height as u32;
            if cursor_x + w + ATLAS_PADDING > ATLAS_WIDTH {
                cursor_x = ATLAS_PADDING;
                cursor_y += row_height + ATLAS_PADDING;
                row_height = 0;
            }
            placements.push((cursor_x, cursor_y));
            cursor_x += w + ATLAS_PADDING;
            row_height = row_height.max(h);
        }
        let atlas_height = (cursor_y + row_height + ATLAS_PADDING).max(1);

        let mut pixels = vec![0u8; (ATLAS_WIDTH * atlas_height) as usize];
        let mut glyphs = HashMap::with_capacity(rasters.len());
        for (r, &(x, y)) in rasters.iter().zip(placements.iter()) {
            let w = r.metrics.width as u32;
            let h = r.metrics.height as u32;
            for row in 0..h {
                let src_start = (row * w) as usize;
                let dst_start = ((y + row) * ATLAS_WIDTH + x) as usize;
                pixels[dst_start..dst_start + w as usize]
                    .copy_from_slice(&r.bitmap[src_start..src_start + w as usize]);
            }
            glyphs.insert(
                r.ch,
                GlyphInfo {
                    uv_min: [
                        x as f32 / ATLAS_WIDTH as f32,
                        y as f32 / atlas_height as f32,
                    ],
                    uv_max: [
                        (x + w) as f32 / ATLAS_WIDTH as f32,
                        (y + h) as f32 / atlas_height as f32,
                    ],
                    width: w as f32,
                    height: h as f32,
                    xmin: r.metrics.xmin as f32,
                    ymin: r.metrics.ymin as f32,
                    advance: r.metrics.advance_width,
                },
            );
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("weft-glyph-atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_WIDTH,
                height: atlas_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
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
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ATLAS_WIDTH),
                rows_per_image: Some(atlas_height),
            },
            wgpu::Extent3d {
                width: ATLAS_WIDTH,
                height: atlas_height,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("weft-glyph-atlas-bind-group"),
            layout: texture_bind_group_layout,
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
        });

        let line_metrics = font
            .horizontal_line_metrics(BASE_PX)
            .unwrap_or(fontdue::LineMetrics {
                ascent: BASE_PX * 0.8,
                descent: -BASE_PX * 0.2,
                line_gap: 0.0,
                new_line_size: BASE_PX,
            });

        Ok(Self {
            bind_group,
            glyphs,
            ascent: line_metrics.ascent,
            line_height: line_metrics.new_line_size,
        })
    }
}

/// Lays out `content` starting at pixel-space `(x, y)` (top-left origin,
/// screen space, y-down) at `size` pixels tall, appending one textured quad
/// per visible glyph to `vertices`/`indices`. `'\n'` starts a new line at
/// the same `x`; no word-wrapping.
#[allow(clippy::too_many_arguments)]
pub(crate) fn layout(
    atlas: &GlyphAtlas,
    content: &str,
    x: f32,
    y: f32,
    size: f32,
    color: [f32; 4],
    vertices: &mut Vec<UiVertex>,
    indices: &mut Vec<u32>,
) {
    let scale = size / BASE_PX;
    let mut pen_x = x;
    let mut pen_y = y;
    for ch in content.chars() {
        if ch == '\n' {
            pen_x = x;
            pen_y += atlas.line_height * scale;
            continue;
        }
        let Some(glyph) = atlas.glyphs.get(&ch) else {
            continue;
        };
        if glyph.width > 0.0 && glyph.height > 0.0 {
            let quad_x = pen_x + glyph.xmin * scale;
            let quad_y = pen_y + (atlas.ascent - glyph.ymin) * scale - glyph.height * scale;
            let quad_w = glyph.width * scale;
            let quad_h = glyph.height * scale;

            let base = vertices.len() as u32;
            vertices.extend_from_slice(&[
                UiVertex {
                    position: [quad_x, quad_y],
                    uv: [glyph.uv_min[0], glyph.uv_min[1]],
                    color,
                },
                UiVertex {
                    position: [quad_x + quad_w, quad_y],
                    uv: [glyph.uv_max[0], glyph.uv_min[1]],
                    color,
                },
                UiVertex {
                    position: [quad_x + quad_w, quad_y + quad_h],
                    uv: [glyph.uv_max[0], glyph.uv_max[1]],
                    color,
                },
                UiVertex {
                    position: [quad_x, quad_y + quad_h],
                    uv: [glyph.uv_min[0], glyph.uv_max[1]],
                    color,
                },
            ]);
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        pen_x += glyph.advance * scale;
    }
}

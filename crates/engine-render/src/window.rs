use std::sync::Arc;

use crate::error::RenderError;
use crate::gpu::{self, GraphicsCore, RenderContext};

/// Live windowed presentation: acquires a swapchain frame, draws into it via
/// the shared `RenderContext`, and presents — no CPU pixel readback at all,
/// unlike `render_scene`/`render_scene_with_context`'s offscreen path. That
/// (not just context reuse) is the perf-relevant difference for a real-time
/// loop.
pub struct WindowRenderer {
    ctx: RenderContext,
    surface: wgpu::Surface<'static>,
    surface_format: wgpu::TextureFormat,
    width: u32,
    height: u32,
}

impl WindowRenderer {
    pub fn new(
        window: Arc<winit::window::Window>,
        width: u32,
        height: u32,
        backends: wgpu::Backends,
    ) -> Result<Self, RenderError> {
        let (core, surface) = GraphicsCore::new_windowed(window, backends)?;
        let caps = surface.get_capabilities(&core.adapter);
        let surface_format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let ctx = RenderContext::from_core(core)?;

        let mut renderer = Self {
            ctx,
            surface,
            surface_format,
            width,
            height,
        };
        renderer.configure_surface();
        Ok(renderer)
    }

    fn configure_surface(&mut self) {
        self.surface.configure(
            self.ctx.device(),
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: self.surface_format,
                color_space: wgpu::SurfaceColorSpace::Auto,
                width: self.width.max(1),
                height: self.height.max(1),
                present_mode: wgpu::PresentMode::Fifo,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            },
        );
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.width = width;
        self.height = height;
        self.configure_surface();
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Extracts the camera/drawables from `world` exactly like
    /// `render_scene` does, draws into the next surface frame, and presents.
    /// `Timeout`/`Occluded` (transient — e.g. a minimized window) are
    /// treated as "skip this frame," not an error; anything else that isn't
    /// a clean acquire is surfaced as a `RenderError` rather than silently
    /// swallowed.
    pub fn render(
        &mut self,
        world: &hecs::World,
        assets_dir: &std::path::Path,
    ) -> Result<(), RenderError> {
        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => t,
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            other => {
                return Err(RenderError::SurfaceAcquireFailed(format!("{other:?}")));
            }
        };
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let command_buffer = gpu::draw_to_surface(
            &mut self.ctx,
            world,
            &view,
            self.surface_format,
            self.width,
            self.height,
            assets_dir,
        )?;
        self.ctx.queue().submit(std::iter::once(command_buffer));
        self.ctx.queue().present(surface_texture);
        Ok(())
    }
}

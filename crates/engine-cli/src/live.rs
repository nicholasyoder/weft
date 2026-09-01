//! The live windowed run loop (`engine play`). Unlike every other command
//! (`run`/`test`/`inspect`/`replay`), this doesn't batch-execute a fixed
//! tick budget — it opens a window and drives a fixed-timestep accumulator
//! against wall-clock time, reading live keyboard state into
//! `engine_core::Input` (see ADR-0010) each frame. `play` takes
//! caller-supplied registries (like `engine_scene::load` already does)
//! rather than hardcoding `registry::components()`/`registry::systems()`
//! the way `SimSource::build` does, so an external consumer (`games/sandbox`)
//! can inject its own extra registrations.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use engine_core::inspect::ComponentDumper;
use engine_core::sim::Sim;
use engine_core::{Input, KeyCode};
use engine_render::WindowRenderer;
use engine_scene::{ComponentRegistry, SystemRegistry};
use engine_script::ScriptHost;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode as WinitKeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::diagnostics::CliError;

/// Loads `scene`, opens a window, and runs it live until the window is
/// closed, Escape is pressed, or (if set) `max_ticks` is reached — the
/// non-interactive escape hatch that makes this command testable in CI/a
/// headless sandbox without a human at the keyboard.
#[allow(clippy::too_many_arguments)]
pub fn play(
    scene: &Path,
    seed: u64,
    assets_dir: &Path,
    components: &ComponentRegistry,
    systems: &SystemRegistry,
    width: u32,
    height: u32,
    backends: wgpu::Backends,
    max_ticks: Option<u64>,
) -> Result<(), CliError> {
    let (sim, dumpers) = engine_scene::load(scene, seed, components, systems)
        .map_err(|e| CliError::from_scene_error(scene, &e))?;
    let host = crate::build_script_host(&sim)?;

    let event_loop =
        EventLoop::new().map_err(|e| CliError::play_event_loop_failed(&e.to_string()))?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App {
        sim,
        dumpers,
        host,
        components,
        assets_dir: assets_dir.to_path_buf(),
        width,
        height,
        backends,
        window: None,
        renderer: None,
        input: Input::default(),
        accumulator: 0.0,
        last_instant: None,
        max_ticks,
        error: None,
    };

    event_loop
        .run_app(&mut app)
        .map_err(|e| CliError::play_event_loop_failed(&e.to_string()))?;

    match app.error.take() {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

struct App<'a> {
    sim: Sim,
    dumpers: Vec<ComponentDumper>,
    host: Option<ScriptHost>,
    components: &'a ComponentRegistry,
    assets_dir: PathBuf,
    width: u32,
    height: u32,
    backends: wgpu::Backends,
    window: Option<Arc<Window>>,
    renderer: Option<WindowRenderer>,
    input: Input,
    /// Wall-clock seconds accumulated but not yet consumed by a fixed
    /// `sim.dt`-sized `Sim::step()` — the standard fixed-timestep
    /// accumulator pattern, which keeps the simulation's own determinism
    /// guarantee (ADR-0002) intact: each `step()` still advances by exactly
    /// `dt`, regardless of real frame timing.
    accumulator: f32,
    last_instant: Option<Instant>,
    max_ticks: Option<u64>,
    error: Option<CliError>,
}

fn map_key(code: WinitKeyCode) -> Option<KeyCode> {
    match code {
        WinitKeyCode::KeyA => Some(KeyCode::A),
        WinitKeyCode::KeyB => Some(KeyCode::B),
        WinitKeyCode::KeyC => Some(KeyCode::C),
        WinitKeyCode::KeyD => Some(KeyCode::D),
        WinitKeyCode::KeyE => Some(KeyCode::E),
        WinitKeyCode::KeyF => Some(KeyCode::F),
        WinitKeyCode::KeyG => Some(KeyCode::G),
        WinitKeyCode::KeyH => Some(KeyCode::H),
        WinitKeyCode::KeyI => Some(KeyCode::I),
        WinitKeyCode::KeyJ => Some(KeyCode::J),
        WinitKeyCode::KeyK => Some(KeyCode::K),
        WinitKeyCode::KeyL => Some(KeyCode::L),
        WinitKeyCode::KeyM => Some(KeyCode::M),
        WinitKeyCode::KeyN => Some(KeyCode::N),
        WinitKeyCode::KeyO => Some(KeyCode::O),
        WinitKeyCode::KeyP => Some(KeyCode::P),
        WinitKeyCode::KeyQ => Some(KeyCode::Q),
        WinitKeyCode::KeyR => Some(KeyCode::R),
        WinitKeyCode::KeyS => Some(KeyCode::S),
        WinitKeyCode::KeyT => Some(KeyCode::T),
        WinitKeyCode::KeyU => Some(KeyCode::U),
        WinitKeyCode::KeyV => Some(KeyCode::V),
        WinitKeyCode::KeyW => Some(KeyCode::W),
        WinitKeyCode::KeyX => Some(KeyCode::X),
        WinitKeyCode::KeyY => Some(KeyCode::Y),
        WinitKeyCode::KeyZ => Some(KeyCode::Z),
        WinitKeyCode::Digit0 => Some(KeyCode::Digit0),
        WinitKeyCode::Digit1 => Some(KeyCode::Digit1),
        WinitKeyCode::Digit2 => Some(KeyCode::Digit2),
        WinitKeyCode::Digit3 => Some(KeyCode::Digit3),
        WinitKeyCode::Digit4 => Some(KeyCode::Digit4),
        WinitKeyCode::Digit5 => Some(KeyCode::Digit5),
        WinitKeyCode::Digit6 => Some(KeyCode::Digit6),
        WinitKeyCode::Digit7 => Some(KeyCode::Digit7),
        WinitKeyCode::Digit8 => Some(KeyCode::Digit8),
        WinitKeyCode::Digit9 => Some(KeyCode::Digit9),
        WinitKeyCode::ArrowUp => Some(KeyCode::Up),
        WinitKeyCode::ArrowDown => Some(KeyCode::Down),
        WinitKeyCode::ArrowLeft => Some(KeyCode::Left),
        WinitKeyCode::ArrowRight => Some(KeyCode::Right),
        WinitKeyCode::Space => Some(KeyCode::Space),
        WinitKeyCode::Enter => Some(KeyCode::Enter),
        WinitKeyCode::Tab => Some(KeyCode::Tab),
        WinitKeyCode::Escape => Some(KeyCode::Escape),
        WinitKeyCode::ShiftLeft => Some(KeyCode::LeftShift),
        WinitKeyCode::ShiftRight => Some(KeyCode::RightShift),
        WinitKeyCode::ControlLeft => Some(KeyCode::LeftControl),
        WinitKeyCode::ControlRight => Some(KeyCode::RightControl),
        WinitKeyCode::AltLeft => Some(KeyCode::LeftAlt),
        WinitKeyCode::AltRight => Some(KeyCode::RightAlt),
        _ => None,
    }
}

impl ApplicationHandler for App<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window = match event_loop.create_window(
            Window::default_attributes()
                .with_inner_size(winit::dpi::PhysicalSize::new(self.width, self.height))
                .with_title("Weft"),
        ) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                self.error = Some(CliError::play_window_init_failed(&e.to_string()));
                event_loop.exit();
                return;
            }
        };

        match WindowRenderer::new(window.clone(), self.width, self.height, self.backends) {
            Ok(renderer) => self.renderer = Some(renderer),
            Err(e) => {
                self.error = Some(CliError::from_render_error(&e));
                event_loop.exit();
                return;
            }
        }

        self.window = Some(window);
        self.last_instant = Some(Instant::now());
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state,
                        ..
                    },
                ..
            } => {
                if code == WinitKeyCode::Escape && state == ElementState::Pressed {
                    event_loop.exit();
                    return;
                }
                if let Some(key) = map_key(code) {
                    self.input.set_held(key, state == ElementState::Pressed);
                }
            }
            WindowEvent::Resized(size) => {
                self.width = size.width;
                self.height = size.height;
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size.width, size.height);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let elapsed = now
            .duration_since(self.last_instant.unwrap_or(now))
            .as_secs_f32();
        self.last_instant = Some(now);
        // Clamp against a "spiral of death" catch-up burst after a real
        // stall (e.g. the window being dragged) — better to visibly slow
        // down than to run an unbounded number of sim ticks in one go.
        self.accumulator += elapsed.min(0.25);

        self.sim.resources.insert(self.input.clone());
        while self.accumulator >= self.sim.dt {
            if let Err(e) = crate::step_and_dispatch_with_input(
                &mut self.sim,
                &self.dumpers,
                self.host.as_mut(),
                self.components,
                &self.input,
            ) {
                self.error = Some(e);
                event_loop.exit();
                return;
            }
            self.accumulator -= self.sim.dt;
            if self.max_ticks.is_some_and(|m| self.sim.tick >= m) {
                event_loop.exit();
                return;
            }
        }

        if let Some(renderer) = &mut self.renderer {
            if let Err(e) = renderer.render(&self.sim.world, &self.assets_dir) {
                self.error = Some(CliError::from_render_error(&e));
                event_loop.exit();
                return;
            }
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

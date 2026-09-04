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
use engine_core::{Input, KeyCode, MouseDelta};
use engine_render::WindowRenderer;
use engine_scene::{ComponentRegistry, SystemRegistry};
use engine_script::ScriptHost;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode as WinitKeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

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
    let (sim, dumpers, host) = build(scene, seed, assets_dir, components, systems)?;

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
        mouse_delta: MouseDelta::default(),
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

/// Builds the `Sim` a live `play` session runs, plus its script host —
/// split out from `play` itself so it's testable without a window/event
/// loop (see `tests::assets_dir_is_inserted` below).
///
/// `AssetsDir` must be inserted here, the same as `SimSource::build` does
/// for every batch command (`crates/engine-cli/src/lib.rs`) — without it,
/// `animation_step`/`audio_step` both silently no-op every tick (by
/// design, see ADR-0015/ADR-0016), so a live `play` session would never
/// actually animate anything or play any sound at all, even with a
/// perfectly good audio device open. This was a real, previously-latent
/// bug: `live::play` never inserted it before, so `engine play`/`cargo run
/// -p sandbox` never produced any audio, on any machine, regardless of
/// device availability — masked because no test ever ran the live loop
/// against a real device and listened.
fn build(
    scene: &Path,
    seed: u64,
    assets_dir: &Path,
    components: &ComponentRegistry,
    systems: &SystemRegistry,
) -> Result<(Sim, Vec<ComponentDumper>, Option<ScriptHost>), CliError> {
    let (mut sim, dumpers) = engine_scene::load(scene, seed, components, systems)
        .map_err(|e| CliError::from_scene_error(scene, &e))?;
    let host = crate::build_script_host(&sim)?;

    sim.resources
        .insert(engine_core::AssetsDir(assets_dir.to_path_buf()));

    // Opens the real audio device once, up front — doesn't need
    // `ActiveEventLoop` the way the window does, so there's no need to
    // wait for `resumed()`. A missing/unavailable device (a real
    // possibility in a headless sandbox) is not fatal: `kira::backend::
    // cpal::Error::NoDefaultOutputDevice` is logged and `play` continues
    // with no `AudioState` inserted at all, so `audio_step` falls back to
    // its tracking-only default (see ADR-0016) instead of crashing the
    // whole session over an unrelated audio device.
    match engine_audio::LiveAudioBackend::new() {
        Ok(backend) => {
            sim.resources.insert(engine_audio::AudioState::with_backend(
                engine_audio::AudioBackend::Live(Box::new(backend)),
            ));
        }
        Err(e) => {
            eprintln!("warning: no audio device available, continuing with no sound ({e})");
        }
    }

    Ok((sim, dumpers, host))
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
    /// Raw mouse motion accumulated since the last frame snapshot was
    /// inserted into `Resources` — reset to zero right after each insert
    /// (see `about_to_wait`), mirroring `input`'s "one snapshot per
    /// rendered frame" cadence rather than per-tick.
    mouse_delta: MouseDelta,
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

        // Grab + hide the cursor for mouse-look: `Locked` (no cursor
        // movement at all, just raw deltas) is preferred, but X11 only
        // supports `Confined` (cursor stays on-screen but does move) —
        // verified against winit 0.30.13's own X11 backend, which
        // hard-errors `Locked` as unsupported. Neither is fatal if it
        // fails (e.g. an unusual platform/compositor) — same "log and
        // keep going" posture as a missing audio device below.
        if window.set_cursor_grab(CursorGrabMode::Locked).is_err() {
            if let Err(e) = window.set_cursor_grab(CursorGrabMode::Confined) {
                eprintln!("warning: cursor grab not supported on this platform, continuing without mouse-look capture ({e})");
            }
        }
        window.set_cursor_visible(false);
        // Discard any synthetic motion the grab call itself may have
        // generated before the first real frame.
        self.mouse_delta = MouseDelta::default();

        self.window = Some(window);
        self.last_instant = Some(Instant::now());
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            self.mouse_delta.dx += dx as f32;
            self.mouse_delta.dy += dy as f32;
        }
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
        self.sim.resources.insert(self.mouse_delta);
        self.mouse_delta = MouseDelta::default();
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test for a real, previously-latent bug: `build` (and
    /// therefore `play`) must insert `AssetsDir`, or `animation_step`/
    /// `audio_step` silently no-op every tick in a live session — no
    /// window/event loop needed to catch this, since it's just a
    /// `Resources` lookup after `build` returns.
    #[test]
    fn assets_dir_is_inserted() {
        let components = crate::registry::components();
        let systems = crate::registry::systems();
        let (sim, _dumpers, _host) = build(
            Path::new("tests/fixtures/scenes/basic.toml"),
            1,
            Path::new("assets"),
            &components,
            &systems,
        )
        .unwrap();

        assert_eq!(
            sim.resources.get::<engine_core::AssetsDir>().map(|a| &a.0),
            Some(&PathBuf::from("assets")),
        );
    }
}

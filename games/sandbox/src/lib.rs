pub mod player_control;

use std::path::Path;

use player_control::PlayerControl;

fn load_player_control(
    v: serde_json::Value,
    b: &mut hecs::EntityBuilder,
) -> Result<(), serde_json::Error> {
    b.add(serde_json::from_value::<PlayerControl>(v)?);
    Ok(())
}

fn dump_player_control(e: &hecs::EntityRef) -> Option<(&'static str, serde_json::Value)> {
    e.get::<&PlayerControl>()
        .map(|p| ("PlayerControl", serde_json::to_value(*p).unwrap()))
}

/// Opens a window and runs `scene` live, with the sandbox's own extended
/// registry (engine-cli's base components/systems plus `PlayerControl`).
/// The one place the registry-building + `engine_cli::live::play` call is
/// joined, so both `main.rs` and `tests/play.rs` share it (same "one core
/// fn, thin wrappers" shape ADR-0007 established for engine-cli itself).
pub fn play(
    scene: &Path,
    assets_dir: &Path,
    max_ticks: Option<u64>,
) -> Result<(), engine_cli::diagnostics::CliError> {
    let mut components = engine_cli::registry::components();
    components.register("PlayerControl", load_player_control, dump_player_control);

    let mut systems = engine_cli::registry::systems();
    systems.register("player_control", player_control::player_control_system);

    engine_cli::live::play(
        scene,
        1,
        assets_dir,
        &components,
        &systems,
        1024,
        768,
        wgpu::Backends::PRIMARY,
        max_ticks,
    )
}

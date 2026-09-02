pub mod camera_follow;
pub mod hud;
pub mod player_control;

use std::path::Path;

use camera_follow::CameraFollow;
use hud::Pickup;
use player_control::PlayerControl;

/// The sandbox's own extended registry: engine-cli's base components/systems
/// plus `PlayerControl`/`CameraFollow`. Exposed separately from `play` so
/// tests can build a `Sim`/dispatch scripts against the exact same
/// registrations the real game uses, without needing a window.
pub fn registry() -> (
    engine_scene::ComponentRegistry,
    engine_scene::SystemRegistry,
) {
    use engine_cli::registry::{dump, load};

    let mut components = engine_cli::registry::components();
    components.register(
        "PlayerControl",
        load::<PlayerControl>,
        dump::<PlayerControl>,
    );
    components.register("CameraFollow", load::<CameraFollow>, dump::<CameraFollow>);
    components.register("Pickup", load::<Pickup>, dump::<Pickup>);

    let mut systems = engine_cli::registry::systems();
    systems.register("player_control", player_control::player_control_system);
    systems.register("camera_follow", camera_follow::camera_follow_system);
    systems.register("hud", hud::hud_system);

    (components, systems)
}

/// Opens a window and runs `scene` live, with the sandbox's own extended
/// registry ([`registry`]). The one place the registry-building +
/// `engine_cli::live::play` call is joined, so both `main.rs` and
/// `tests/play.rs` share it (same "one core fn, thin wrappers" shape
/// ADR-0007 established for engine-cli itself).
pub fn play(
    scene: &Path,
    assets_dir: &Path,
    max_ticks: Option<u64>,
) -> Result<(), engine_cli::diagnostics::CliError> {
    let (components, systems) = registry();

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

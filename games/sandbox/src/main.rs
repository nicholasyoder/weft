use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut scene = None;
    let mut max_ticks = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--max-ticks" => {
                max_ticks = args
                    .next()
                    .and_then(|v| v.parse::<u64>().ok())
                    .or(max_ticks);
            }
            other => scene = Some(PathBuf::from(other)),
        }
    }
    // Resolve a caller-supplied scene path against the *original* working
    // directory before pinning CWD below — otherwise a relative path typed
    // at the shell would resolve against the wrong directory.
    let scene = match scene {
        Some(path) => match std::path::absolute(&path) {
            Ok(abs) => abs,
            Err(e) => {
                eprintln!("failed to resolve scene path '{}': {e}", path.display());
                return ExitCode::FAILURE;
            }
        },
        None => manifest_dir.join("scenes/playground.toml"),
    };
    let assets_dir = manifest_dir.join("assets");

    // A scene's `Script.path` fields are plain filesystem paths, resolved
    // relative to the process's working directory (`ScriptHost` has no
    // scene-relative path concept) — pin CWD to this crate's own root so
    // `cargo run -p sandbox` behaves the same regardless of which directory
    // it's invoked from, matching the crate-relative convention every
    // script path in this repo already uses (see games/sandbox/scripts/).
    if let Err(e) = std::env::set_current_dir(&manifest_dir) {
        eprintln!(
            "failed to set working directory to '{}': {e}",
            manifest_dir.display()
        );
        return ExitCode::FAILURE;
    }

    match sandbox::play(&scene, &assets_dir, max_ticks) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            e.print(false);
            ExitCode::FAILURE
        }
    }
}

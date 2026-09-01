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
    let scene = scene.unwrap_or_else(|| manifest_dir.join("scenes/playground.toml"));
    let assets_dir = manifest_dir.join("assets");

    match sandbox::play(&scene, &assets_dir, max_ticks) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            e.print(false);
            ExitCode::FAILURE
        }
    }
}

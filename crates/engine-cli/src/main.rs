use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use engine_cli::commands::{self, OutputFormat};
use engine_cli::SimSource;

#[derive(Parser)]
#[command(name = "engine", about = "Weft engine CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a scenario or scene twice and verify it produces byte-identical world state.
    Test {
        #[arg(long, conflicts_with = "scene")]
        scenario: Option<String>,
        #[arg(long, conflicts_with = "scenario")]
        scene: Option<PathBuf>,
        #[arg(long, default_value_t = 1)]
        seed: u64,
        #[arg(long, default_value_t = 60)]
        ticks: u64,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
    /// Load a scene file, run it for N ticks, and exit. With --watch,
    /// instead reruns the same budget from scratch whenever the scene file
    /// or a referenced Lua script changes, until the process is killed.
    Run {
        scene: PathBuf,
        #[arg(long, default_value_t = 1)]
        seed: u64,
        #[arg(long, default_value_t = 60)]
        ticks: u64,
        #[arg(long)]
        watch: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
    /// Load a scene file, run it for N ticks, and render the final world
    /// state to a PNG. No window or display server required.
    Render {
        scene: PathBuf,
        #[arg(long)]
        to: PathBuf,
        #[arg(long, default_value = "assets")]
        assets_dir: PathBuf,
        #[arg(long, default_value_t = 1)]
        seed: u64,
        #[arg(long, default_value_t = 60)]
        ticks: u64,
        #[arg(long, default_value_t = 256)]
        width: u32,
        #[arg(long, default_value_t = 256)]
        height: u32,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
    /// Convert a glTF file or a loose image file into the content-addressed
    /// asset store, emitting a scene-text-file fragment ready to paste in.
    Import {
        input: PathBuf,
        #[arg(long, default_value = "assets")]
        assets_dir: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
    /// Dump a scenario's or scene's final world state as JSON.
    Inspect {
        #[command(flatten)]
        source: InspectSource,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
    /// Replay a recording file deterministically.
    Replay {
        recording: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
}

#[derive(Args)]
#[group(required = false, multiple = true)]
struct InspectSource {
    #[arg(long, conflicts_with_all = ["recording", "scene"])]
    scenario: Option<String>,
    #[arg(long, default_value_t = 1)]
    seed: u64,
    #[arg(long, default_value_t = 60)]
    ticks: u64,
    #[arg(long, conflicts_with_all = ["scenario", "scene"])]
    recording: Option<PathBuf>,
    #[arg(long, conflicts_with_all = ["scenario", "recording"])]
    scene: Option<PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Test {
            scenario,
            scene,
            seed,
            ticks,
            format,
        } => {
            let source = match scene {
                Some(path) => SimSource::Scene(path),
                None => SimSource::Scenario(scenario.unwrap_or_else(|| "basic".to_string())),
            };
            commands::test::run(source, seed, ticks, format)
        }
        Command::Run {
            scene,
            seed,
            ticks,
            watch,
            format,
        } => commands::run::run(&scene, seed, ticks, watch, format),
        Command::Render {
            scene,
            to,
            assets_dir,
            seed,
            ticks,
            width,
            height,
            format,
        } => commands::render::run(&scene, &to, &assets_dir, seed, ticks, width, height, format),
        Command::Import {
            input,
            assets_dir,
            out,
            format,
        } => commands::import::run(&input, &assets_dir, out.as_deref(), format),
        Command::Inspect { source, format } => {
            let src = if let Some(path) = source.recording {
                commands::inspect::Source::Recording { path }
            } else if let Some(path) = source.scene {
                commands::inspect::Source::Inline {
                    source: SimSource::Scene(path),
                    seed: source.seed,
                    ticks: source.ticks,
                }
            } else {
                commands::inspect::Source::Inline {
                    source: SimSource::Scenario(
                        source.scenario.unwrap_or_else(|| "basic".to_string()),
                    ),
                    seed: source.seed,
                    ticks: source.ticks,
                }
            };
            commands::inspect::run(src, format)
        }
        Command::Replay { recording, format } => commands::replay::run(&recording, format),
    }
}

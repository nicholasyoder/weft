use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use engine_cli::commands::{self, OutputFormat};

#[derive(Parser)]
#[command(name = "engine", about = "Weft engine CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a scenario twice and verify it produces byte-identical world state.
    Test {
        #[arg(long, default_value = "basic")]
        scenario: String,
        #[arg(long, default_value_t = 1)]
        seed: u64,
        #[arg(long, default_value_t = 60)]
        ticks: u64,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
    /// Dump a scenario's final world state as JSON.
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
    #[arg(long)]
    scenario: Option<String>,
    #[arg(long, default_value_t = 1)]
    seed: u64,
    #[arg(long, default_value_t = 60)]
    ticks: u64,
    #[arg(long, conflicts_with_all = ["scenario"])]
    recording: Option<PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Test {
            scenario,
            seed,
            ticks,
            format,
        } => commands::test::run(&scenario, seed, ticks, format),
        Command::Inspect { source, format } => {
            let src = match source.recording {
                Some(path) => commands::inspect::Source::Recording { path },
                None => commands::inspect::Source::Inline {
                    scenario: source.scenario.unwrap_or_else(|| "basic".to_string()),
                    seed: source.seed,
                    ticks: source.ticks,
                },
            };
            commands::inspect::run(src, format)
        }
        Command::Replay { recording, format } => commands::replay::run(&recording, format),
    }
}

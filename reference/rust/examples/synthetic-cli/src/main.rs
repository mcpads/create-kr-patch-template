mod target;
mod translation;

use std::{fs, path::PathBuf};

use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand, ValueEnum};
use patch_guard::{BuildMode, sha256_hex};
use target::BuildResult;

#[derive(Debug, Parser)]
#[command(name = "synthetic-cli")]
#[command(about = "Self-authored example; replace this CLI and target model as needed")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create the self-authored binary used by this example.
    MakeSource {
        #[arg(long)]
        output: PathBuf,
    },
    /// Build through the complete product graph.
    Build {
        #[arg(long, value_enum)]
        mode: CliBuildMode,
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        translations: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        report: PathBuf,
    },
    /// Rebuild in memory and require byte-identical equality with an output file.
    Verify {
        #[arg(long, value_enum)]
        mode: CliBuildMode,
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        translations: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliBuildMode {
    Development,
    ReleaseCandidate,
}

impl From<CliBuildMode> for BuildMode {
    fn from(value: CliBuildMode) -> Self {
        match value {
            CliBuildMode::Development => Self::Development,
            CliBuildMode::ReleaseCandidate => Self::ReleaseCandidate,
        }
    }
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::MakeSource { output } => {
            let source = target::demo_source();
            write_file(&output, &source)?;
            println!(
                "created={} len={} sha256={}",
                output.display(),
                source.len(),
                sha256_hex(&source)
            );
        }
        Command::Build {
            mode,
            source,
            translations,
            output,
            report,
        } => {
            let result = load_and_build(&source, &translations, mode.into())?;
            write_file(&output, &result.output)?;
            write_file(&report, &serde_json::to_vec_pretty(&result.report)?)?;
            print_summary(&result);
            println!("output={} report={}", output.display(), report.display());
        }
        Command::Verify {
            mode,
            source,
            translations,
            output,
        } => {
            let result = load_and_build(&source, &translations, mode.into())?;
            let actual =
                fs::read(&output).with_context(|| format!("read output {}", output.display()))?;
            ensure!(
                actual == result.output,
                "output {} differs from a fresh product build",
                output.display()
            );
            print_summary(&result);
            println!("verified={}", output.display());
        }
    }
    Ok(())
}

fn load_and_build(
    source: &PathBuf,
    translations: &PathBuf,
    mode: BuildMode,
) -> Result<BuildResult> {
    let source_bytes =
        fs::read(source).with_context(|| format!("read source {}", source.display()))?;
    let translation_bytes = fs::read(translations)
        .with_context(|| format!("read translations {}", translations.display()))?;
    target::build(&source_bytes, &translation_bytes, mode)
}

fn write_file(path: &PathBuf, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create output directory {}", parent.display()))?;
    }
    fs::write(path, bytes).with_context(|| format!("write {}", path.display()))
}

fn print_summary(result: &BuildResult) {
    println!(
        "mode={:?} release_candidate={} unresolved={} source_sha256={} output_sha256={} writes={}",
        result.report.mode,
        result.report.release_candidate,
        result.report.readiness.unresolved_units.len(),
        result.report.source_inputs[0].sha256,
        result.report.output.sha256,
        result.report.writes.len()
    );
}

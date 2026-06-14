mod commands;
mod config;
mod hooks;

use anyhow::Result;
use clap::{Parser, Subcommand};
use commands::{
    backup::BackupCommand, check::CheckCommand, copy::CopyCommand, diff::DiffCommand,
    dump::DumpCommand, forget::ForgetCommand, init::InitCommand, job::JobCommand, ls::LsCommand,
    prune::PruneCommand, restore::RestoreCommand, snapshots::SnapshotsCommand,
    stats::StatsCommand,
};
use tracing::info;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

#[derive(Parser)]
#[command(
    name = "ghostsnap",
    about = "A production-grade backup tool",
    long_about = "Ghostsnap is a fast, secure, and efficient backup tool with deduplication support"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(long, env = "GHOSTSNAP_REPO", help = "Repository path")]
    repo: Option<String>,

    #[arg(long, env = "GHOSTSNAP_PASSWORD", help = "Repository password")]
    password: Option<String>,

    #[arg(short, long, help = "Enable verbose output")]
    verbose: bool,

    #[arg(short, long, help = "Enable quiet mode")]
    quiet: bool,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Initialize a new repository")]
    Init(InitCommand),

    #[command(about = "Create a new backup")]
    Backup(BackupCommand),

    #[command(about = "List snapshots")]
    Snapshots(SnapshotsCommand),

    #[command(about = "Restore files from a snapshot")]
    Restore(RestoreCommand),

    #[command(about = "Show repository statistics")]
    Stats(StatsCommand),

    #[command(about = "Check repository integrity")]
    Check(CheckCommand),

    #[command(about = "List files in a snapshot")]
    Ls(LsCommand),

    #[command(about = "Apply retention policies to snapshots")]
    Forget(ForgetCommand),

    #[command(about = "Remove unused data and reclaim space")]
    Prune(PruneCommand),

    #[command(about = "Compare two snapshots")]
    Diff(DiffCommand),

    #[command(about = "Extract a file from a snapshot to stdout")]
    Dump(DumpCommand),

    #[command(about = "Copy snapshots between repositories")]
    Copy(CopyCommand),

    #[command(about = "Run config-driven backup jobs")]
    Job(JobCommand),
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    init_tracing(cli.verbose, cli.quiet);

    info!("Starting Ghostsnap");

    match cli.command {
        Commands::Init(ref cmd) => cmd.run(&cli).await,
        Commands::Backup(ref cmd) => cmd.run(&cli).await,
        Commands::Snapshots(ref cmd) => cmd.run(&cli).await,
        Commands::Restore(ref cmd) => cmd.run(&cli).await,
        Commands::Stats(ref cmd) => cmd.run(&cli).await,
        Commands::Check(ref cmd) => cmd.run(&cli).await,
        Commands::Ls(ref cmd) => cmd.run(&cli).await,
        Commands::Forget(ref cmd) => cmd.run(&cli).await,
        Commands::Prune(ref cmd) => cmd.run(&cli).await,
        Commands::Diff(ref cmd) => cmd.run(&cli).await,
        Commands::Dump(ref cmd) => cmd.run(&cli).await,
        Commands::Copy(ref cmd) => cmd.run(&cli).await,
        Commands::Job(ref cmd) => cmd.run(&cli).await,
    }
}

fn init_tracing(verbose: bool, quiet: bool) {
    let level = if quiet {
        "warn"
    } else if verbose {
        "debug"
    } else {
        "info"
    };

    let subscriber = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::new(format!("ghostsnap={}", level)))
        .finish();

    // Ignore errors: a global subscriber may already be set (e.g. when the CLI
    // is exercised from multiple integration tests in the same process).
    let _ = tracing::subscriber::set_global_default(subscriber);
}

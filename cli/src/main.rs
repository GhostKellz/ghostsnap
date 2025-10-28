mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};
use commands::{init::InitCommand, backup::BackupCommand, snapshots::SnapshotsCommand, hestia::HestiaCommand, restore::RestoreCommand};
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
    
    #[command(about = "HestiaCP integration commands")]
    Hestia(HestiaCommand),
    
    #[command(about = "Restore files from a snapshot")]
    Restore {
        #[arg(help = "Snapshot ID to restore from")]
        snapshot_id: String,
        
        #[arg(help = "Target directory for restore")]
        target: String,
        
        #[arg(help = "Specific paths to restore")]
        paths: Vec<String>,
    },
    
    #[command(about = "Show repository statistics")]
    Stats,
    
    #[command(about = "Check repository integrity")]
    Check,
    
    #[command(about = "Remove unused data and apply retention policies")]
    Forget {
        #[arg(long, help = "Keep last N snapshots")]
        keep_last: Option<u32>,
        
        #[arg(long, help = "Keep daily snapshots for N days")]
        keep_daily: Option<u32>,
        
        #[arg(long, help = "Keep weekly snapshots for N weeks")]
        keep_weekly: Option<u32>,
        
        #[arg(long, help = "Keep monthly snapshots for N months")]
        keep_monthly: Option<u32>,
        
        #[arg(long, help = "Actually remove data (dry-run otherwise)")]
        prune: bool,
    },
    
    #[command(about = "List files in a snapshot")]
    Ls {
        #[arg(help = "Snapshot ID")]
        snapshot_id: String,
        
        #[arg(help = "Path within snapshot")]
        path: Option<String>,
    },
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
        Commands::Hestia(ref cmd) => cmd.run(&cli).await,
        Commands::Restore { ref snapshot_id, ref target, ref paths } => {
            RestoreCommand::run(snapshot_id.clone(), target.clone(), paths.clone(), &cli).await
        },
        Commands::Stats => {
            println!("Stats not yet implemented");
            Ok(())
        },
        Commands::Check => {
            println!("Check not yet implemented");
            Ok(())
        },
        Commands::Forget { .. } => {
            println!("Forget not yet implemented");
            Ok(())
        },
        Commands::Ls { snapshot_id: _, path: _ } => {
            println!("Ls not yet implemented");
            Ok(())
        },
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
    
    tracing::subscriber::set_global_default(subscriber)
        .expect("Setting default subscriber failed");
}
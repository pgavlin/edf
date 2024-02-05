use clap::{Args, Parser, Subcommand, ValueEnum};
use std::error::Error;

mod common;
mod dump;
mod io;
mod mk;
mod show;

use dump::dump;
use mk::mk;
use show::show;

#[derive(Debug, Parser)]
#[command(name = "edf")]
#[command(about = "A CLI for working with edf documents", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Dump(DumpArgs),
    Mk(MkArgs),
    Show(ShowArgs),
}

#[derive(Debug, Args)]
struct MkArgs {
    #[arg(index = 1, required = false)]
    input_path: Option<String>,

    #[arg(long, required = false, value_enum)]
    format: Option<MkFormat>,

    #[arg(short = 'c', required = false)]
    format_config: Option<String>,

    #[arg(short, required = true)]
    device_config: String,

    #[arg(short, required = false)]
    font_config: Option<String>,

    #[arg(short, required = false)]
    output_path: Option<String>,
}

#[derive(Debug, Clone, ValueEnum)]
enum MkFormat {
    Markdown,

    #[cfg(feature = "epub")]
    Epub,
}

#[derive(Debug, Args)]
struct DumpArgs {
    #[arg(index = 1, required = false)]
    input_path: Option<String>,
}

#[derive(Debug, Args)]
struct ShowArgs {
    #[arg(index = 1, required = false)]
    input_path: Option<String>,

    #[arg(short, required = true)]
    device_config: String,

    #[arg(short, required = false)]
    font_config: Option<String>,

    #[arg(short, required = true)]
    page_num: u32,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Cli::parse();

    match args.command {
        Commands::Dump(args) => dump(args),
        Commands::Mk(args) => mk(args),
        Commands::Show(args) => show(args),
    }
}

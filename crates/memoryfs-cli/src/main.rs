use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "memoryfs", about = "Compile and lint Git-backed agent memory")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the workspace version.
    Version,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Version => println!("{}", memoryfs_core::version()),
    }
}

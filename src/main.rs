use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod index;
mod plugin;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

/// The validator's subcommands. To be able to also add scanning functionality later (because why
/// not?), validate is a subcommand.
#[derive(Subcommand)]
enum Commands {
    /// Validate one or more plugins.
    Validate {
        /// Paths to one or more plugins that should be validated.
        #[clap(value_parser, required(true))]
        paths: Vec<PathBuf>,
        /// Run the tests within this process.
        ///
        /// Tests are normally run in separate processes in case the plugin crashes. Another benefit
        /// of the out of process validation is that the test always starts from a clean state.
        /// Using this option will remove those protections, but in turn the tests may run faster.
        #[clap(value_parser, short, long)]
        in_process: bool,
        /// Print the test output as JSON instead of human readable text.
        #[clap(value_parser, short, long)]
        json: bool,
    },
    // TODO: A hidden subcommand for running a single test for a single plugin. Used by the out of
    //       process mode
    /// Lists basic information about all installed CLAP plugins.
    List {
        /// Print JSON instead of a human readable format.
        #[clap(value_parser, short, long)]
        json: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Validate { .. } => {
            todo!("Implement the validator")
        }
        Commands::List { json } => {
            //
        }
    }
}
use std::process::ExitCode;

use apibump::cli::{run, Cli};
use clap::Parser;

fn main() -> ExitCode {
    let cli = Cli::parse();

    match run(cli) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("apibump: {error}");
            ExitCode::from(2)
        }
    }
}

//! Standalone .tx texture converter using OIIO.
//!
//! Runs as a subprocess so OIIO crashes don't take down the main app.
//! Usage: bif_maketx <input_path> <output_path>

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: bif_maketx <input> <output>");
        return ExitCode::from(1);
    }

    let input = &args[1];
    let output = &args[2];

    match bif_core::oiio::make_tx(input, output, None) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("bif_maketx error: {e}");
            ExitCode::from(2)
        }
    }
}

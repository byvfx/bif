// Phase A / B dogfood binary for bif_qt.
//
// Lets us iterate on the Qt shell without touching bif_viewer yet.
// Phase F deletes this binary + switches bif_viewer over.
//
//   . .\setup_qt_env.ps1
//   cargo run -p bif_qt --bin bif_qt_shell

fn main() {
    match bif_qt::run() {
        Ok(rc) => std::process::exit(rc),
        Err(e) => {
            eprintln!("bif_qt_shell failed: {e:#}");
            std::process::exit(1);
        }
    }
}

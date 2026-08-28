//! Native Windows installer for the GeneralsOnline community patch data.
//!
//! This is a Rust port of the original Python installer script. It
//! downloads the mod data archive from GitHub, extracts it, and installs
//! it into the user's Documents folder, replacing any existing
//! installation if the user confirms. See the project write-up for a
//! full function-by-function comparison against the Python source.
//!
//! Developed by Abdulrahman
//! GitHub: github.com/abdulrahman20242

mod archive;
mod config;
mod download;
mod error;
mod installer;
mod paths;

use std::path::PathBuf;
use std::process::ExitCode;

use error::InstallError;
use installer::{confirm_existing_installation, run_installation_pipeline};

fn main() -> ExitCode {
    println!("GeneralsOnline Data Installer");
    println!("Developed by Abdulrahman - github.com/abdulrahman20242");
    println!();

    // Mirrors Python's `confirm_existing_installation()` call in main():
    // if the user declines to replace an existing install, that is a
    // clean, successful exit (Python: `sys.exit(0)`), not a failure.
    if !confirm_existing_installation() {
        return ExitCode::SUCCESS;
    }

    // Mirrors `temp_folder = Path(tempfile.mkdtemp(prefix="generals_mod_"))`.
    // Nothing has been created on disk yet at this point (same as the
    // Python script), so there is nothing to clean up if this step itself
    // fails.
    let temp_folder = match create_temp_folder() {
        Ok(path) => path,
        Err(e) => {
            println!("{e}");
            return ExitCode::FAILURE;
        }
    };

    // Mirrors `zip_path = temp_folder / f"{MOD_FOLDER_NAME}.zip"`.
    let zip_path = temp_folder.join(format!("{}.zip", config::MOD_FOLDER_NAME));

    if run_installation_pipeline(&temp_folder, &zip_path) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Create a uniquely named temporary directory and detach it from
/// `tempfile`'s automatic drop-cleanup, so its lifetime is managed
/// explicitly by `installer::cleanup_temporary_files` -- exactly
/// mirroring `tempfile.mkdtemp()`'s Python semantics (created once,
/// removed explicitly later), rather than relying on Rust-side RAII.
///
/// Review finding: a permission-denied failure here used to fall into
/// the generic `Other` message like any other cause. Classified the same
/// way the rest of the filesystem-facing code now is, for consistency.
fn create_temp_folder() -> Result<PathBuf, InstallError> {
    tempfile::Builder::new()
        .prefix(config::TEMP_DIR_PREFIX)
        .tempdir()
        .map(|dir| dir.keep())
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                InstallError::PermissionDenied
            } else {
                InstallError::Other(e.to_string())
            }
        })
}

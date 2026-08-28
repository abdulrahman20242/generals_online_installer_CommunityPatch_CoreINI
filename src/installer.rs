//! Installation steps and the top-level pipeline that runs them in order.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::archive::extract_archive;
use crate::config::{DESTINATION_FOLDER, DOWNLOAD_URL, MOD_FOLDER_NAME, VERIFY_FILENAME};
use crate::download::download_archive_with_retries;
use crate::error::InstallError;
use crate::paths::resolve_documents_folder;

/// True if the mod's verification file is already present at its expected
/// final location.
fn core_ini_already_installed() -> bool {
    let documents_folder = resolve_documents_folder();
    let target_file = documents_folder
        .join(DESTINATION_FOLDER)
        .join(MOD_FOLDER_NAME)
        .join(VERIFY_FILENAME);
    target_file.exists()
}

/// Ask the user whether to replace an existing installation.
///
/// Any answer other than "y" or "yes" (case-insensitive) counts as "no",
/// matching the Python script's `user_input.strip().lower() in ("y",
/// "yes")`. A read failure (e.g. stdin closed / EOF) is also treated as
/// "no", so that nothing destructive happens by default.
fn confirm_file_overwrite() -> bool {
    print!("An existing '{MOD_FOLDER_NAME}' installation was found. Replace it? (y/n): ");
    let _ = io::stdout().flush();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return false;
    }

    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}

/// Windows' `MoveFileExW` (which `fs::rename` uses under the hood) fails
/// with `ERROR_NOT_SAME_DEVICE` (raw code 17) for a cross-volume rename.
/// `ErrorKind::CrossesDevices` is std's stable, cross-platform mapping for
/// exactly that condition; the raw-code check is kept alongside it only
/// as a defensive fallback in case a given toolchain's mapping ever
/// differs.
const ERROR_NOT_SAME_DEVICE: i32 = 17;

fn is_cross_device_error(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::CrossesDevices || err.raw_os_error() == Some(ERROR_NOT_SAME_DEVICE)
}

/// Move `source_folder` into `destination_parent`, replacing anything
/// already there under the same name.
///
/// The existing destination is removed first so the move lands directly
/// at `destination_parent/<source name>` instead of nesting inside a
/// pre-existing folder of the same name -- the same reason the Python
/// version calls `shutil.rmtree(destination_path)` before `shutil.move`.
///
/// `fs::rename` is tried first (fast, atomic, same-volume). Its failure
/// is then classified rather than assumed:
/// - a genuine cross-volume failure falls back to a recursive copy +
///   delete (a redirected Documents folder can legitimately live on a
///   different drive than the system temp directory);
/// - a permission failure is reported as `PermissionDenied`;
/// - anything else is reported as `MoveFailed` carrying the *original*
///   `rename` error text -- it is not masked behind a copy attempt that
///   would most likely just fail again for the same underlying reason.
fn move_folder(source_folder: &Path, destination_parent: &Path) -> Result<PathBuf, InstallError> {
    let destination_path = destination_parent.join(
        source_folder
            .file_name()
            .expect("extraction target always has a file name"),
    );

    if destination_path.exists() {
        fs::remove_dir_all(&destination_path).map_err(map_fs_error)?;
    }

    if let Err(rename_err) = fs::rename(source_folder, &destination_path) {
        if is_cross_device_error(&rename_err) {
            copy_dir_all(source_folder, &destination_path)
                .and_then(|_| fs::remove_dir_all(source_folder))
                .map_err(map_fs_error)?;
        } else if rename_err.kind() == io::ErrorKind::PermissionDenied {
            return Err(InstallError::PermissionDenied);
        } else {
            return Err(InstallError::MoveFailed(rename_err.to_string()));
        }
    }

    Ok(destination_path)
}

/// Recursively copy a directory tree. Only used as `move_folder`'s
/// cross-volume fallback; std has no built-in "move directory across
/// filesystems" helper the way `shutil.move` provides in Python.
fn copy_dir_all(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dest_path = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &dest_path)?;
        } else {
            fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

fn map_fs_error(err: io::Error) -> InstallError {
    if err.kind() == io::ErrorKind::PermissionDenied {
        InstallError::PermissionDenied
    } else {
        InstallError::MoveFailed(err.to_string())
    }
}

/// Distinct from `map_fs_error` on purpose: a failure creating the
/// destination directory happens *before* any move is attempted, so
/// labeling it as a move failure would misdescribe what actually went
/// wrong. Permission-denied handling is preserved -- that specific case
/// still gets the existing `PermissionDenied` message.
fn map_destination_creation_error(err: io::Error) -> InstallError {
    if err.kind() == io::ErrorKind::PermissionDenied {
        InstallError::PermissionDenied
    } else {
        InstallError::DestinationCreationFailed(err.to_string())
    }
}

/// Confirm the verification file exists inside the installed mod folder,
/// printing a clear success or failure message either way (both messages
/// are printed by the Python version regardless of the exit machinery, so
/// the success line is printed directly here rather than deferred).
fn verify_core_ini(mod_folder_path: &Path) -> Result<(), InstallError> {
    let verification_target = mod_folder_path.join(VERIFY_FILENAME);
    if verification_target.exists() {
        println!(
            "\u{2705} Verified: '{VERIFY_FILENAME}' exists at {}",
            verification_target.display()
        );
        return Ok(());
    }

    Err(InstallError::VerificationFailed {
        expected_path: verification_target,
    })
}

/// Remove the downloaded ZIP and the temporary extraction directory.
///
/// Always called, even when a preceding step failed -- mirroring the
/// Python script's `finally: cleanup_temporary_files(...)`. A failure
/// here is reported but does not override the outcome of the
/// installation itself; by this point the user already knows whether the
/// install succeeded, and a leftover temp file is a minor, separate
/// problem worth a warning rather than a second fatal error.
fn cleanup_temporary_files(zip_path: &Path, temp_folder: &Path) {
    if zip_path.exists() {
        if let Err(e) = fs::remove_file(zip_path) {
            eprintln!(
                "\nWarning: could not remove temporary file {}: {e}",
                zip_path.display()
            );
        }
    }
    if temp_folder.exists() {
        if let Err(e) = fs::remove_dir_all(temp_folder) {
            eprintln!(
                "\nWarning: could not remove temporary folder {}: {e}",
                temp_folder.display()
            );
        }
    }
}

/// Check for an existing installation and, if the user does not want to
/// replace it, print a message and signal that the program should stop.
///
/// Returns `true` if installation should proceed, `false` if the user
/// chose to keep the existing installation (the Python equivalent of
/// this branch is `sys.exit(0)`: a clean, successful exit, not a
/// failure).
pub fn confirm_existing_installation() -> bool {
    println!("Checking existing installation...");
    if !core_ini_already_installed() {
        return true;
    }
    if !confirm_file_overwrite() {
        println!("Skipped. Existing installation kept.");
        return false;
    }
    println!("Replacing existing installation...");
    true
}

/// Run the full download -> extract -> resolve destination -> move ->
/// verify pipeline, printing the same "Step N/5" progress lines as the
/// Python script. Returns `true` on success, `false` on failure (the
/// failure message has already been printed by the time this returns).
///
/// Temporary files are always cleaned up before returning, mirroring the
/// Python script's `finally` block: cleanup runs whether the pipeline
/// above succeeded or exited early through `?`.
pub fn run_installation_pipeline(temp_folder: &Path, zip_path: &Path) -> bool {
    let result = run_pipeline_steps(temp_folder, zip_path);

    if let Err(ref e) = result {
        println!("{e}");
    }

    cleanup_temporary_files(zip_path, temp_folder);

    result.is_ok()
}

fn run_pipeline_steps(temp_folder: &Path, zip_path: &Path) -> Result<(), InstallError> {
    println!("Step 1/5: Downloading mod archive...");
    download_archive_with_retries(DOWNLOAD_URL, zip_path)?;

    println!("Step 2/5: Extracting archive...");
    let extraction_target = extract_archive(zip_path, temp_folder)?;

    println!("Step 3/5: Resolving destination path...");
    let documents_folder = resolve_documents_folder();
    let destination_parent = documents_folder.join(DESTINATION_FOLDER);
    fs::create_dir_all(&destination_parent).map_err(map_destination_creation_error)?;

    println!("Step 4/5: Moving mod files to Documents...");
    let installed_path = move_folder(&extraction_target, &destination_parent)?;

    println!("Step 5/5: Verifying installation...");
    verify_core_ini(&installed_path)?;

    println!("Done.");
    Ok(())
}

//! ZIP extraction and structure validation.

use std::fs::File;
use std::path::{Path, PathBuf};

use zip::result::ZipError;

use crate::config::MOD_FOLDER_NAME;
use crate::error::InstallError;

/// Extract `zip_path` into `temp_folder`, then confirm the expected
/// top-level mod folder is present and return its path.
///
/// Mirrors `extract_archive_or_exit` in the Python script: a structural
/// ZIP problem (`zipfile.BadZipFile`) and a missing expected folder are
/// reported with distinct, specific messages.
pub fn extract_archive(zip_path: &Path, temp_folder: &Path) -> Result<PathBuf, InstallError> {
    let file =
        File::open(zip_path).map_err(|e| InstallError::ExtractionFailed(e.to_string()))?;
    let mut archive = zip::ZipArchive::new(file).map_err(map_zip_error)?;
    archive.extract(temp_folder).map_err(map_zip_error)?;

    let extraction_target = temp_folder.join(MOD_FOLDER_NAME);
    if !extraction_target.exists() {
        return Err(InstallError::UnexpectedArchiveStructure);
    }

    Ok(extraction_target)
}

/// A structurally invalid/corrupt archive gets the specific "not a valid
/// ZIP archive" message; any other extraction-time problem (e.g. an I/O
/// error writing an extracted entry) gets the generic
/// "failed to extract" message with the underlying reason attached.
fn map_zip_error(err: ZipError) -> InstallError {
    match err {
        ZipError::InvalidArchive(_) | ZipError::UnsupportedArchive(_) => InstallError::InvalidZip,
        ZipError::Io(io_err) => InstallError::ExtractionFailed(io_err.to_string()),
        other => InstallError::ExtractionFailed(other.to_string()),
    }
}

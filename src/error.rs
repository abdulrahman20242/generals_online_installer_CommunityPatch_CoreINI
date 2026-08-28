//! The installer's single error type.
//!
//! Every variant's [`std::fmt::Display`] implementation produces the exact
//! user-facing message for that failure category. Call sites print the
//! error once (via `{e}`) and then decide the process exit code -- they
//! never need to know the wording.
//!
//! Message design note: the request that specified this wording gave
//! three slightly different phrasings of the same "connection failed"
//! case across its own sections 1, 3, and 12. This file treats section 3
//! ("Download Errors") as the canonical text for what each category says,
//! and prints that in full on every failed download attempt -- not just
//! the shortened one-liner shown in section 12's abbreviated transcript --
//! since section 2's stated goal ("the user must understand what actually
//! failed") is better served by the complete reason every time than a
//! trimmed version reserved only for the final attempt.

use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum InstallError {
    /// A connection to the download server could not be established at
    /// all (DNS failure, refused connection, dropped socket before any
    /// response). Retried.
    ConnectionFailed,

    /// The connection or response did not arrive within the allotted
    /// time. Retried.
    Timeout,

    /// The server responded with an HTTP error status. Retried.
    HttpStatus(u16),

    /// The response body ended (cleanly, from the reader's point of view)
    /// before as many bytes arrived as the server's `Content-Length`
    /// promised -- a connection cut mid-transfer that didn't surface as
    /// an outright I/O error. Retried.
    IncompleteDownload,

    /// All download attempts were exhausted without success. Carries the
    /// total attempt count so the message stays correct if
    /// `config::MAX_DOWNLOAD_ATTEMPTS` ever changes.
    DownloadFailedAfterAttempts { attempts: u32 },

    /// The downloaded file was structurally not a valid ZIP archive
    /// (corrupt / truncated / wrong format). Not retried -- a successful
    /// download that isn't a valid archive won't fix itself by retrying.
    InvalidZip,

    /// Extraction failed for a reason other than an invalid archive
    /// format -- e.g. an I/O error opening the downloaded file or writing
    /// extracted entries to disk. Not retried.
    ExtractionFailed(String),

    /// The archive extracted successfully but did not contain the
    /// expected top-level mod folder. Not retried.
    UnexpectedArchiveStructure,

    /// An operation on the destination folder failed with an OS
    /// permissions error. Not retried.
    PermissionDenied,

    /// The destination directory itself could not be created or accessed,
    /// for a reason other than a permissions error (that case stays
    /// `PermissionDenied`). Distinct from `MoveFailed` because this
    /// happens before any move is attempted. Not retried.
    DestinationCreationFailed(String),

    /// Moving the extracted files into place failed for a reason other
    /// than a permissions error. Not retried.
    MoveFailed(String),

    /// The verification file was missing after installation. Not
    /// retried.
    VerificationFailed { expected_path: PathBuf },

    /// Any other I/O or runtime error that does not fit the categories
    /// above. Nothing is left to propagate as a raw, unhandled dump or a
    /// silent exit instead. Not retried.
    Other(String),
}

impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstallError::ConnectionFailed => write!(
                f,
                "Error: Unable to connect to the internet.\nPlease check your internet connection and try again."
            ),
            InstallError::Timeout => write!(
                f,
                "Error: The download timed out.\nPlease check your internet connection and try again."
            ),
            InstallError::HttpStatus(code) => write!(
                f,
                "Error: Failed to download the required files.\nThe server returned HTTP status {code}."
            ),
            InstallError::IncompleteDownload => write!(
                f,
                "Error: The download was interrupted before the file was completely downloaded."
            ),
            InstallError::DownloadFailedAfterAttempts { attempts } => write!(
                f,
                "Error: Download failed after {attempts} attempts.\nPlease check your internet connection and try again."
            ),
            InstallError::InvalidZip => write!(
                f,
                "Error: The downloaded file is not a valid ZIP archive.\nPlease try the installation again."
            ),
            InstallError::ExtractionFailed(reason) => write!(
                f,
                "Error: Failed to extract the downloaded archive.\nThe ZIP file may be corrupted or incomplete.\nPlease try the installation again.\n(Reason: {reason})"
            ),
            InstallError::UnexpectedArchiveStructure => write!(
                f,
                "Error: The downloaded archive has an unexpected structure.\nThe expected installation folder was not found."
            ),
            InstallError::PermissionDenied => write!(
                f,
                "Error: Permission denied while accessing the installation folder.\nPlease close any application using this folder and try again."
            ),
            InstallError::DestinationCreationFailed(reason) => write!(
                f,
                "Error: Could not create or access the installation destination.\nPlease make sure the destination folder is accessible.\n(Reason: {reason})"
            ),
            InstallError::MoveFailed(reason) => write!(
                f,
                "Error: Failed to move the installation files.\nPlease make sure the destination folder is accessible.\n(Reason: {reason})"
            ),
            InstallError::VerificationFailed { expected_path } => write!(
                f,
                "Error: Installation verification failed.\nThe required file was not found:\n\n{}\n\nExpected at: {}\nThis can happen if the download or archive was incomplete or corrupted -- try running the installer again.",
                crate::config::VERIFY_FILENAME,
                expected_path.display()
            ),
            InstallError::Other(reason) => write!(
                f,
                "Error: An unexpected error occurred.\n{reason}\nPlease try the installation again."
            ),
        }
    }
}

impl std::error::Error for InstallError {}

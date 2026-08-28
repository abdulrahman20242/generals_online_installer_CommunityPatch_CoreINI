//! Fixed configuration values for the installer.
//!
//! These mirror the module-level constants at the top of the original
//! Python script exactly. Per the migration spec, `DOWNLOAD_URL`,
//! `MOD_FOLDER_NAME`, `VERIFY_FILENAME`, and `DESTINATION_FOLDER` must not
//! change without a concrete technical reason.

/// Direct download URL for the mod data archive.
pub const DOWNLOAD_URL: &str = "https://github.com/ReizanTech/Additional-content-in-Command-Conquer-Generals-Zero-Hour/releases/download/has-modified-INI-files/GeneralsOnlineGameData.zip";

/// Name of the folder at the root of the extracted archive, and the name
/// the installed folder keeps once moved into place.
pub const MOD_FOLDER_NAME: &str = "GeneralsOnlineGameData";

/// File used to confirm that an installation is present / completed
/// successfully.
pub const VERIFY_FILENAME: &str = "500_900_CommunityPatch_CoreINI.big";

/// Folder created inside the user's Documents directory that holds the
/// installed mod folder.
pub const DESTINATION_FOLDER: &str = "Command and Conquer Generals Zero Hour Data";

/// Prefix used for the temporary working directory, matching
/// `tempfile.mkdtemp(prefix="generals_mod_")` in the Python script.
pub const TEMP_DIR_PREFIX: &str = "generals_mod_";

/// Chunk size used when streaming the download to disk, matching the
/// Python script's `chunk_size = 8192`.
pub const DOWNLOAD_CHUNK_SIZE: usize = 8192;

/// Maximum number of download attempts before giving up: 1 initial
/// attempt plus up to 4 retries.
pub const MAX_DOWNLOAD_ATTEMPTS: u32 = 5;

/// Delay between a failed download attempt and the next one, in seconds.
/// Not applied after the final attempt.
pub const RETRY_DELAY_SECS: u64 = 2;

//! Resolving the user's Windows Documents folder.

use std::env;
use std::path::PathBuf;

use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

/// Resolve the user's Documents folder.
///
/// Mirrors the Python implementation exactly: read the `Personal` value
/// from the legacy `Shell Folders` registry key first. That key (as
/// opposed to `User Shell Folders`) stores an already-expanded absolute
/// path and correctly reflects a redirected/relocated Documents folder
/// (e.g. moved to another drive, or redirected by OneDrive or Group
/// Policy), which a hard-coded `home/Documents` guess would miss.
///
/// Falls back to `%USERPROFILE%\Documents` if the registry read fails for
/// any reason, matching Python's `except (ImportError, OSError):
/// return Path.home() / "Documents"`.
pub fn resolve_documents_folder() -> PathBuf {
    if let Some(path) = read_personal_shell_folder() {
        return PathBuf::from(path);
    }

    let home = env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join("Documents")
}

fn read_personal_shell_folder() -> Option<String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let shell_folders = hkcu
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders")
        .ok()?;
    let personal: String = shell_folders.get_value("Personal").ok()?;
    Some(personal)
}

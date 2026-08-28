//! Streaming the mod archive download to disk, with progress reporting
//! and retry logic for transient network failures.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::thread;
use std::time::Duration;

use crate::config::{DOWNLOAD_CHUNK_SIZE, MAX_DOWNLOAD_ATTEMPTS, RETRY_DELAY_SECS};
use crate::error::InstallError;

/// Download the archive, retrying network-category failures (connection
/// failure / timeout / HTTP status) up to `MAX_DOWNLOAD_ATTEMPTS` times
/// with a fixed `RETRY_DELAY_SECS`-second delay between attempts. No
/// delay follows the final attempt, and any partial download is removed
/// before the next attempt starts.
///
/// A failure that is *not* network-category (e.g. the local temp file
/// couldn't be created) is not retried: retrying a network request
/// cannot fix a local filesystem problem, so that case fails immediately
/// instead of consuming the remaining attempts.
pub fn download_archive_with_retries(
    url: &str,
    destination_path: &Path,
) -> Result<(), InstallError> {
    for attempt in 1..=MAX_DOWNLOAD_ATTEMPTS {
        println!("Download attempt {attempt}/{MAX_DOWNLOAD_ATTEMPTS}...");

        match download_file(url, destination_path) {
            Ok(()) => return Ok(()),

            Err(e) if is_retryable(&e) => {
                println!("{e}");
                remove_partial_download(destination_path);

                if attempt < MAX_DOWNLOAD_ATTEMPTS {
                    println!("Retrying in {RETRY_DELAY_SECS} seconds...");
                    println!();
                    thread::sleep(Duration::from_secs(RETRY_DELAY_SECS));
                }
            }

            Err(e) => {
                remove_partial_download(destination_path);
                return Err(e);
            }
        }
    }

    Err(InstallError::DownloadFailedAfterAttempts {
        attempts: MAX_DOWNLOAD_ATTEMPTS,
    })
}

/// Only genuinely transient, retry-worthy categories get retried.
/// `HttpStatus` is retried only for the 5xx server-error range (500-599):
/// a 5xx can be a transient server hiccup, but a 4xx (400, 401, 403, 404,
/// etc.) means the request itself won't succeed no matter how many times
/// it's repeated, so those fail on the first attempt instead of wasting
/// four retries and eight seconds. A filesystem-flavored failure from
/// inside a single attempt (see `download_file`'s local-write error
/// mapping) falls through to the non-retryable arm in the caller above.
fn is_retryable(err: &InstallError) -> bool {
    matches!(
        err,
        InstallError::ConnectionFailed
            | InstallError::Timeout
            | InstallError::IncompleteDownload
            | InstallError::HttpStatus(500..=599)
    )
}

fn remove_partial_download(path: &Path) {
    if path.exists() {
        if let Err(e) = std::fs::remove_file(path) {
            eprintln!(
                "Warning: could not remove partial download {}: {e}",
                path.display()
            );
        }
    }
}

/// Review finding (not one of the three named fixes, but the same class
/// of issue): writing the temp download file used to fall into the
/// generic `Other` bucket even when the real cause was permission-denied
/// on the temp directory. Classified the same way `map_fs_error` and
/// `map_destination_creation_error` classify their own local-filesystem
/// failures, for a consistent, specific message instead of a generic one.
fn map_local_write_error(err: io::Error) -> InstallError {
    if err.kind() == io::ErrorKind::PermissionDenied {
        InstallError::PermissionDenied
    } else {
        InstallError::Other(err.to_string())
    }
}

/// Perform a single download attempt: stream the response body to disk
/// in fixed-size chunks (never buffering the whole file in memory) and
/// print a progress indicator as it goes. Called only by
/// `download_archive_with_retries`, which owns the retry policy above.
///
/// Reaching EOF on the body reader is not, by itself, treated as proof
/// the file is complete: a connection can be cut after part of the body
/// has arrived and still surface as a clean `Ok(0)` read rather than an
/// I/O error. Whenever the server declared a `Content-Length`, the
/// actual bytes written are compared against it after the loop ends, and
/// a mismatch is reported as `IncompleteDownload` (retryable) rather
/// than treated as success. If no `Content-Length` was provided, this
/// check is skipped -- there is no reliable total to compare against, so
/// the existing "EOF means done" behavior is left exactly as it was.
fn download_file(url: &str, destination_path: &Path) -> Result<(), InstallError> {
    let agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(60)))
        .timeout_recv_response(Some(Duration::from_secs(60)))
        .build()
        .new_agent();

    let mut response = agent.get(url).call().map_err(map_ureq_error)?;

    let expected_size = response.body().content_length();
    let mut reader = response.body_mut().as_reader();

    let mut file = File::create(destination_path).map_err(map_local_write_error)?;

    let mut buffer = [0u8; DOWNLOAD_CHUNK_SIZE];
    let mut downloaded: u64 = 0;

    loop {
        let bytes_read = reader.read(&mut buffer).map_err(map_io_error)?;
        if bytes_read == 0 {
            break;
        }

        file.write_all(&buffer[..bytes_read])
            .map_err(map_local_write_error)?;

        downloaded += bytes_read as u64;
        print_progress(downloaded, expected_size);
    }

    println!(); // move past the in-place progress line

    if let Some(expected) = expected_size {
        if downloaded != expected {
            return Err(InstallError::IncompleteDownload);
        }
    }

    Ok(())
}

fn print_progress(downloaded: u64, total: Option<u64>) {
    let mut stdout = io::stdout();
    match total {
        Some(total) if total > 0 => {
            let percent = (downloaded as f64 / total as f64 * 100.0).min(100.0);
            let _ = write!(
                stdout,
                "\rDownloading: {percent:5.1}%  {}/{}",
                human_readable(downloaded),
                human_readable(total)
            );
        }
        _ => {
            let _ = write!(stdout, "\rDownloading: {}", human_readable(downloaded));
        }
    }
    let _ = stdout.flush();
}

/// Formats a byte count the same way tqdm's `unit_scale=True` does:
/// decimal (1000-based) B/kB/MB/GB/TB, one decimal place above the first
/// unit. This is a visual approximation of tqdm's bar, not a
/// byte-for-byte reproduction -- see Behavioral Differences.
fn human_readable(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit: usize = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes}{}", UNITS[0])
    } else {
        format!("{value:.1}{}", UNITS[unit])
    }
}

/// Classification is based on ureq 3.4.0's actual `Error` enum (fetched
/// from source, not assumed from memory -- this enum has changed shape
/// across major ureq versions before). Each arm below is a decision, not
/// a default:
///
/// - `HostNotFound` / `ConnectionFailed`: DNS failure / connection
///   failure. Retryable -- classic transient connectivity problems.
/// - `Protocol`: ureq's own doc comment names "invalid chunked transfer"
///   as a cause. That is the chunked-encoding equivalent of a truncated
///   Content-Length transfer -- data stopped arriving mid-stream and the
///   parser noticed, rather than the socket noticing. Routed to the same
///   `IncompleteDownload` category as a Content-Length mismatch, since
///   it is, in substance, the same failure.
/// - `Io`: delegated to `map_io_error` below.
/// - Left non-retryable, each for a specific reason: `BadUri` and `Http`
///   (malformed URL/request -- would fail identically every time);
///   `Tls`, `Pem`, `Rustls`, `NativeTls`, `Der`, `TlsRequired`
///   (certificate/TLS configuration -- retrying cannot fix a bad cert);
///   `InvalidProxyUrl`, `ConnectProxyFailed`, `RedirectFailed`,
///   `BodyExceedsLimit`, `RequireHttpsOnly` (structural mismatches that
///   do not apply to this GET-only, no-proxy download, and would not
///   change on retry regardless); `TooManyRedirects`, `LargeResponseHeader`,
///   `Decompress` (possible in principle, but nothing in ureq's docs
///   points at these specifically indicating a transient condition the
///   way `Protocol`'s "invalid chunked transfer" does -- left
///   conservative rather than guessed retryable).
fn map_ureq_error(err: ureq::Error) -> InstallError {
    match err {
        ureq::Error::StatusCode(code) => InstallError::HttpStatus(code),
        ureq::Error::Timeout(_) => InstallError::Timeout,
        ureq::Error::Io(io_err) => map_io_error(io_err),
        ureq::Error::HostNotFound | ureq::Error::ConnectionFailed => {
            InstallError::ConnectionFailed
        }
        ureq::Error::Protocol(_) => InstallError::IncompleteDownload,
        other => InstallError::Other(other.to_string()),
    }
}

/// `TimedOut` / `UnexpectedEof` / the connection-state kinds are the
/// original set. Added this review: `HostUnreachable`, `NetworkUnreachable`,
/// `NetworkDown` (routing-level connectivity failures, same character as
/// `ConnectionRefused`/`ConnectionAborted` -- confirmed stable well before
/// this project's toolchain, in Rust 1.83.0), `BrokenPipe` (the
/// connection died while sending, e.g. mid-redirect -- long-stable, not
/// part of the same stabilization batch as the others here), and
/// `Interrupted` (std's own documentation for this kind says such
/// operations "can typically be retried"; rare on Windows specifically,
/// included for correctness rather than expected frequency).
fn map_io_error(err: io::Error) -> InstallError {
    use io::ErrorKind;
    match err.kind() {
        ErrorKind::TimedOut => InstallError::Timeout,
        ErrorKind::UnexpectedEof => InstallError::IncompleteDownload,
        ErrorKind::ConnectionRefused
        | ErrorKind::ConnectionReset
        | ErrorKind::ConnectionAborted
        | ErrorKind::NotConnected
        | ErrorKind::AddrNotAvailable
        | ErrorKind::HostUnreachable
        | ErrorKind::NetworkUnreachable
        | ErrorKind::NetworkDown
        | ErrorKind::BrokenPipe
        | ErrorKind::Interrupted => InstallError::ConnectionFailed,
        ErrorKind::PermissionDenied => InstallError::PermissionDenied,
        _ => InstallError::Other(err.to_string()),
    }
}

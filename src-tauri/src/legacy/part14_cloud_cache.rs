use std::fs::OpenOptions;

const CLOUD_ARCHIVE_CACHE_LIMIT: u64 = 3 * 1024 * 1024 * 1024;
const DIAGNOSTIC_LOG_LIMIT: u64 = 1_000_000;
const DIAGNOSTIC_LOG_GENERATIONS: u8 = 4;

fn diagnostic_log_directory() -> Result<PathBuf, String> {
    let profiles = profile_store_base()?;
    let manager = profiles.parent().ok_or("CCM log path is invalid.")?;
    Ok(manager.join("logs"))
}

fn diagnostic_log_path() -> Result<PathBuf, String> {
    Ok(diagnostic_log_directory()?.join("ccm-reborn.log"))
}

/// Logging is intentionally best-effort: a full or read-only home directory
/// must not make a campaign installation fail.
fn append_diagnostic_log(entry: &str) {
    let Ok(directory) = diagnostic_log_directory() else { return; };
    if fs::create_dir_all(&directory).is_err() { return; }
    let Ok(path) = diagnostic_log_path() else { return; };
    if fs::metadata(&path).map(|metadata| metadata.len() >= DIAGNOSTIC_LOG_LIMIT).unwrap_or(false) {
        for generation in (1..=DIAGNOSTIC_LOG_GENERATIONS).rev() {
            let from = if generation == 1 {
                path.clone()
            } else {
                path.with_extension(format!("log.{}", generation - 1))
            };
            let to = path.with_extension(format!("log.{generation}"));
            let _ = fs::remove_file(&to);
            let _ = fs::rename(from, to);
        }
    }
    let Ok(mut log) = OpenOptions::new().create(true).append(true).open(path) else { return; };
    let _ = writeln!(log, "{} {}", unix_timestamp(), entry.replace(['\r', '\n'], " "));
}

fn redact_diagnostic_value(value: &str) -> String {
    let mut redacted = value.replace(['\r', '\n'], " ");
    for variable in ["HOME", "USERPROFILE"] {
        if let Some(home) = std::env::var_os(variable) {
            let home = PathBuf::from(home).display().to_string();
            if !home.is_empty() {
                redacted = redacted.replace(&home, "~");
            }
        }
    }
    redacted
}

#[tauri::command]
fn get_diagnostic_log_path() -> Result<String, String> {
    diagnostic_log_path().map(|path| path.display().to_string())
}

#[tauri::command]
async fn open_diagnostic_log_directory() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        let directory = diagnostic_log_directory()?;
        fs::create_dir_all(&directory).map_err(io_error)?;
        #[cfg(target_os = "windows")]
        let mut command = Command::new("explorer");
        #[cfg(target_os = "macos")]
        let mut command = Command::new("open");
        #[cfg(all(unix, not(target_os = "macos")))]
        let mut command = Command::new("xdg-open");
        command.arg(&directory).spawn()
            .map_err(|error| format!("Could not open the CCM log folder: {error}"))?;
        Ok(())
    })
    .await
    .map_err(|error| format!("Log-folder worker failed: {error}"))?
}

fn cloud_cache_directory(category: &str) -> Result<PathBuf, String> {
    let profiles = profile_store_base()?;
    let manager = profiles.parent().ok_or("CCM cache path is invalid.")?;
    let home = manager.parent().ok_or("CCM cache path is invalid.")?;
    let mut current = home.to_path_buf();
    for component in [".ccm-reborn", "cache", category] {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(format!("CCM cache component {} must be a regular directory.", current.display()));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(io_error)?;
                let metadata = fs::symlink_metadata(&current).map_err(io_error)?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(format!("CCM cache component {} must be a regular directory.", current.display()));
                }
            }
            Err(error) => return Err(io_error(error)),
        }
    }
    Ok(current)
}

fn cloud_cache_key(value: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(value.as_bytes());
    hex::encode(digest.finalize())
}

fn catalog_cache_path(source: &str) -> Result<PathBuf, String> {
    Ok(cloud_cache_directory("catalogs")?.join(format!("{}.json", cloud_cache_key(source))))
}

fn read_cached_catalog(source: &str, max_size: u64) -> Result<String, String> {
    let path = catalog_cache_path(source)?;
    let metadata = fs::symlink_metadata(&path).map_err(|_| "No cached cloud catalog is available.".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > max_size {
        return Err("Cached cloud catalog is invalid.".into());
    }
    String::from_utf8(fs::read(path).map_err(io_error)?)
        .map_err(|_| "Cached cloud catalog is not UTF-8 JSON.".into())
}

fn cache_catalog(source: &str, catalog: &str) -> Result<(), String> {
    write_text_atomic(&catalog_cache_path(source)?, catalog)
}

fn archive_cache_path(expected_sha256: &str) -> Result<PathBuf, String> {
    Ok(cloud_cache_directory("archives")?.join(format!("{expected_sha256}.zip")))
}

fn cache_remote_archive(
    url: &str,
    expected_sha256: &str,
    expected_size: u64,
    reporter: Option<&ProgressReporter>,
) -> Result<PathBuf, String> {
    if expected_size > MAX_ARCHIVE_BYTES {
        return Err("Package is larger than the allowed download size.".into());
    }
    let path = archive_cache_path(expected_sha256)?;
    reporter.map(|reporter| reporter.status("checking-cache", "Checking the verified download cache…"));
    if cached_archive_matches(&path, expected_sha256, expected_size)? {
        reporter.map(|reporter| reporter.status("cache-hit", "Using a verified cached package — no download is needed."));
        return Ok(path);
    }
    for attempt in 1..=2 {
        reporter.map(|reporter| reporter.status(
            "downloading",
            if attempt == 1 { "Downloading package from cloud…" } else { "Connection was interrupted; retrying download (2/2)…" },
        ));
        let temporary = path.with_extension(format!("{}.part", Uuid::new_v4()));
        let result = download_remote_archive_once(url, expected_sha256, expected_size, &path, &temporary, reporter);
        match result {
            Ok(()) => {
                prune_archive_cache(path.parent().ok_or("CCM archive cache path is invalid.")?, &path);
                return Ok(path);
            }
            Err(CloudDownloadFailure::Retryable(error)) if attempt == 1 => {
                let _ = fs::remove_file(&temporary);
                append_diagnostic_log(&format!("download retry scheduled: {}", redact_diagnostic_value(&error)));
            }
            Err(error) => {
                let _ = fs::remove_file(&temporary);
                return Err(error.into_message());
            }
        }
    }
    Err("Could not download package from cloud. No files were changed.".into())
}

enum CloudDownloadFailure {
    Retryable(String),
    Permanent(String),
}

impl CloudDownloadFailure {
    fn into_message(self) -> String {
        match self {
            Self::Retryable(message) | Self::Permanent(message) => message,
        }
    }
}

fn download_remote_archive_once(
    url: &str,
    expected_sha256: &str,
    expected_size: u64,
    path: &Path,
    temporary: &Path,
    reporter: Option<&ProgressReporter>,
) -> Result<(), CloudDownloadFailure> {
    let response = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| CloudDownloadFailure::Permanent(format!("Could not create cloud download client: {error}")))?
        .get(url)
        .header(reqwest::header::ACCEPT_ENCODING, "identity")
        .send()
        .map_err(|error| CloudDownloadFailure::Retryable(format!("Could not connect to the cloud download: {error}")))?
        .error_for_status()
        .map_err(|error| {
            let detail = error.status().map(|status| format!("HTTP {status}")).unwrap_or_else(|| error.to_string());
            CloudDownloadFailure::Permanent(format!("Cloud download was rejected by the server ({detail})."))
        })?;
    if response.content_length().is_some_and(|size| size != expected_size) {
        return Err(CloudDownloadFailure::Permanent("The package size does not match the catalog entry. No files were changed.".into()));
    }
    let result = (|| -> Result<(), CloudDownloadFailure> {
        let mut output = File::create(&temporary)
            .map_err(|error| CloudDownloadFailure::Permanent(io_error(error)))?;
        let mut reader = response.take(MAX_ARCHIVE_BYTES + 1);
        let mut digest = Sha256::new();
        let mut copied = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = reader.read(&mut buffer).map_err(|error| CloudDownloadFailure::Retryable(format!(
                "Could not read the cloud download response: {error}. No files were changed."
            )))?;
            if read == 0 { break; }
            copied = copied.saturating_add(read as u64);
            if copied > MAX_ARCHIVE_BYTES { return Err(CloudDownloadFailure::Permanent("Package is larger than the allowed download size.".into())); }
            digest.update(&buffer[..read]);
            output.write_all(&buffer[..read]).map_err(|error| CloudDownloadFailure::Permanent(io_error(error)))?;
            reporter.map(|reporter| reporter.download(copied, expected_size));
        }
        output.sync_all().map_err(|error| CloudDownloadFailure::Permanent(io_error(error)))?;
        drop(output);
        if copied != expected_size {
            return Err(CloudDownloadFailure::Permanent("The package size does not match the catalog entry. No files were changed.".into()));
        }
        if hex::encode(digest.finalize()) != expected_sha256 {
            return Err(CloudDownloadFailure::Permanent("The downloaded archive does not match the catalog SHA-256. Installation stopped.".into()));
        }
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(CloudDownloadFailure::Permanent("CCM archive cache destination is not a regular file.".into()));
            }
        }
        atomic_replace(&temporary, &path).map_err(CloudDownloadFailure::Permanent)
    })();
    if result.is_err() { let _ = fs::remove_file(&temporary); }
    result
}

fn cached_archive_matches(path: &Path, expected_sha256: &str, expected_size: u64) -> Result<bool, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(io_error(error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("CCM archive cache entry is not a regular file.".into());
    }
    Ok(metadata.len() == expected_size && sha256_file(path)? == expected_sha256)
}

fn prune_archive_cache(directory: &Path, keep: &Path) {
    let mut entries = collect_regular_files(directory).unwrap_or_default().into_iter()
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?;
            let valid = name.len() == 68 && name.ends_with(".zip")
                && name[..64].bytes().all(|byte| byte.is_ascii_hexdigit());
            valid.then(|| fs::metadata(&path).ok().map(|metadata| (metadata.modified().ok(), metadata.len(), path))).flatten()
        })
        .collect::<Vec<_>>();
    let mut total = entries.iter().map(|(_, size, _)| *size).sum::<u64>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    for (_, size, path) in entries {
        if total <= CLOUD_ARCHIVE_CACHE_LIMIT { break; }
        if path != keep && fs::remove_file(&path).is_ok() { total = total.saturating_sub(size); }
    }
}

#[cfg(test)]
mod cloud_cache_tests {
    use super::*;

    #[test]
    fn cached_archive_must_match_the_catalog_size_and_hash() {
        let root = std::env::temp_dir().join(format!("ccm-cloud-cache-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("archive.zip");
        fs::write(&archive, b"verified archive").unwrap();
        let hash = sha256_file(&archive).unwrap();
        assert!(cached_archive_matches(&archive, &hash, 16).unwrap());
        assert!(!cached_archive_matches(&archive, &hash, 15).unwrap());
        fs::write(&archive, b"changed archive!").unwrap();
        assert!(!cached_archive_matches(&archive, &hash, 16).unwrap());
        let _ = fs::remove_dir_all(root);
    }
}

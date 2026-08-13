const CLOUD_ARCHIVE_CACHE_LIMIT: u64 = 3 * 1024 * 1024 * 1024;

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

fn cache_remote_archive(url: &str, expected_sha256: &str, expected_size: u64) -> Result<PathBuf, String> {
    if expected_size > MAX_ARCHIVE_BYTES {
        return Err("Package is larger than the allowed download size.".into());
    }
    let path = archive_cache_path(expected_sha256)?;
    if cached_archive_matches(&path, expected_sha256, expected_size)? {
        return Ok(path);
    }
    let response = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| format!("Could not create download client: {error}"))?
        .get(url)
        .send()
        .map_err(|error| format!("Could not download package: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Package download failed: {error}"))?;
    if response.content_length().is_some_and(|size| size != expected_size) {
        return Err("The package size does not match the catalog entry. No files were changed.".into());
    }
    let temporary = path.with_extension(format!("{}.part", Uuid::new_v4()));
    let result = (|| {
        let mut output = File::create(&temporary).map_err(io_error)?;
        let mut reader = response.take(MAX_ARCHIVE_BYTES + 1);
        let mut digest = Sha256::new();
        let mut copied = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = reader.read(&mut buffer).map_err(io_error)?;
            if read == 0 { break; }
            copied = copied.saturating_add(read as u64);
            if copied > MAX_ARCHIVE_BYTES { return Err("Package is larger than the allowed download size.".into()); }
            digest.update(&buffer[..read]);
            output.write_all(&buffer[..read]).map_err(io_error)?;
        }
        output.sync_all().map_err(io_error)?;
        drop(output);
        if copied != expected_size {
            return Err("The package size does not match the catalog entry. No files were changed.".into());
        }
        if hex::encode(digest.finalize()) != expected_sha256 {
            return Err("The downloaded archive does not match the catalog SHA-256. Installation stopped.".into());
        }
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err("CCM archive cache destination is not a regular file.".into());
            }
        }
        atomic_replace(&temporary, &path)
    })();
    if result.is_err() { let _ = fs::remove_file(&temporary); }
    result?;
    prune_archive_cache(path.parent().ok_or("CCM archive cache path is invalid.")?, &path);
    Ok(path)
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

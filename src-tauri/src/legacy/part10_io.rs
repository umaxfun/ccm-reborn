struct AcquiredArchive {
    path: PathBuf,
    is_temporary: bool,
    checksum_verified: bool,
}

impl Drop for AcquiredArchive {
    fn drop(&mut self) {
        if self.is_temporary {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn acquire_archive(source: &str, expected_sha256: &str, expected_size: Option<u64>) -> Result<AcquiredArchive, String> {
    let source = source.trim();
    if !source.starts_with("https://") {
        let path = PathBuf::from(source);
        if !path.is_file() {
            return Err("The local package declared in catalog.json was not found.".into());
        }
        return Ok(AcquiredArchive {
            path,
            is_temporary: false,
            checksum_verified: false,
        });
    }
    if let Some(expected_size) = expected_size {
        return Ok(AcquiredArchive {
            path: cache_remote_archive(source, expected_sha256, expected_size)?,
            is_temporary: false,
            checksum_verified: true,
        });
    }
    let path = download_archive(source)?;
    Ok(AcquiredArchive {
        path,
        is_temporary: true,
        checksum_verified: false,
    })
}

fn download_archive(url: &str) -> Result<PathBuf, String> {
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
    if response.content_length().is_some_and(|size| size > MAX_ARCHIVE_BYTES) {
        return Err("Package is larger than the allowed download size.".into());
    }

    let temporary_dir = std::env::temp_dir().join("ccm-reborn");
    fs::create_dir_all(&temporary_dir).map_err(io_error)?;
    let path = temporary_dir.join(format!("{}.zip", Uuid::new_v4()));
    let mut output = File::create(&path).map_err(io_error)?;
    let mut limited = response.take(MAX_ARCHIVE_BYTES + 1);
    let copied = io::copy(&mut limited, &mut output).map_err(io_error)?;
    output.sync_all().map_err(io_error)?;
    if copied > MAX_ARCHIVE_BYTES {
        let _ = fs::remove_file(&path);
        return Err("Package is larger than the allowed download size.".into());
    }
    Ok(path)
}

fn download_text(url: &str, max_size: u64) -> Result<String, String> {
    let response = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| format!("Could not create catalog client: {error}"))?
        .get(url)
        .send()
        .map_err(|error| format!("Could not download catalog: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Catalog download failed: {error}"))?;
    if response.content_length().is_some_and(|size| size > max_size) {
        return Err("Catalog is larger than the allowed size.".into());
    }
    let mut bytes = Vec::new();
    let mut limited = response.take(max_size + 1);
    limited.read_to_end(&mut bytes).map_err(io_error)?;
    if bytes.len() as u64 > max_size {
        return Err("Catalog is larger than the allowed size.".into());
    }
    String::from_utf8(bytes).map_err(|_| "Catalog must be UTF-8 JSON.".into())
}

fn normalize_sha256(value: &str) -> Result<String, String> {
    let hash = value.trim().to_ascii_lowercase();
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Catalog entry has no valid SHA-256 for this package.".into());
    }
    Ok(hash)
}

fn validate_declared_package_size(expected: Option<u64>, actual: u64) -> Result<(), String> {
    if let Some(expected) = expected {
        if expected != actual {
            return Err("The package size does not match the catalog entry. No files were changed.".into());
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let file = File::open(path).map_err(io_error)?;
    sha256_reader(file)
}

fn sha256_reader(mut reader: impl Read) -> Result<String, String> {
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer).map_err(io_error)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(hex::encode(hash.finalize()))
}

fn copy_file(from: &Path, to: &Path) -> Result<(), String> {
    if !from.is_file() {
        return Err(format!("Expected file {} is missing.", from.display()));
    }
    if let Ok(metadata) = fs::symlink_metadata(to) {
        if metadata.file_type().is_symlink() {
            return Err(format!("Refusing to replace symlink {}.", to.display()));
        }
        if !metadata.is_file() {
            return Err(format!("Destination {} is not a regular file.", to.display()));
        }
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    let mut source = File::open(from).map_err(io_error)?;
    let mut target = File::create(to).map_err(io_error)?;
    io::copy(&mut source, &mut target).map_err(io_error)?;
    target.sync_all().map_err(io_error)
}


fn clear_directory(path: &Path) -> Result<(), String> {
    if path.exists() {
        if !path.is_dir() {
            return Err(format!("Campaign target {} is not a directory.", path.display()));
        }
        fs::remove_dir_all(path).map_err(io_error)?;
    }
    fs::create_dir_all(path).map_err(io_error)
}

/// Build a game-relative path only after proving that every existing ancestor
/// between the selected root and the leaf is a normal directory.  Checking
/// only the final leaf is insufficient: a `Maps` or `Mods` symlink can make a
/// seemingly safe relative package path escape the game installation.
fn safe_game_path(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    let relative = safe_relative_path(&path_string(relative))?;
    let root_type = fs::symlink_metadata(root)
        .map_err(io_error)?
        .file_type();
    if root_type.is_symlink() || !root_type.is_dir() {
        return Err("Selected game directory is not a regular directory.".into());
    }
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err("Game path contains an invalid component.".into());
        };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!("Refusing to use symlinked game path {}.", current.display()));
            }
            Ok(metadata) if current != root.join(&relative) && !metadata.is_dir() => {
                return Err(format!("Game path ancestor {} is not a directory.", current.display()));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(error)),
        }
    }
    Ok(root.join(relative))
}

fn collect_campaign_target_files(root: &Path, target: &str) -> Result<Vec<PathBuf>, String> {
    let target = safe_campaign_root(target)?;
    let path = safe_game_path(root, &target)?;
    if target != Path::new("Maps/Campaign") {
        return collect_regular_files(&path);
    }
    if !path.exists() {
        return Ok(Vec::new());
    }
    if !path.is_dir() {
        return Err(format!("Campaign target {} is not a directory.", path.display()));
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(&path).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let entry_path = entry.path();
        let file_type = entry.file_type().map_err(io_error)?;
        if file_type.is_symlink() {
            return Err(format!("Refusing to back up symlink {}.", entry_path.display()));
        }
        if file_type.is_dir() && is_wol_owned_asset_directory(&entry.file_name()) {
            collect_regular_files_inner(&entry_path, &mut files)?;
        } else if file_type.is_file() {
            files.push(entry_path);
        }
    }
    Ok(files)
}

fn clear_campaign_target(root: &Path, target: &str) -> Result<(), String> {
    let target = safe_campaign_root(target)?;
    let path = safe_game_path(root, &target)?;
    if target != Path::new("Maps/Campaign") {
        return clear_directory(&path);
    }
    if !path.exists() {
        return fs::create_dir_all(&path).map_err(io_error);
    }
    if !path.is_dir() {
        return Err(format!("Campaign target {} is not a directory.", path.display()));
    }
    for entry in fs::read_dir(&path).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let entry_path = entry.path();
        let file_type = entry.file_type().map_err(io_error)?;
        if file_type.is_symlink() {
            return Err(format!("Refusing to clear symlink {}.", entry_path.display()));
        }
        if file_type.is_dir() && is_wol_owned_asset_directory(&entry.file_name()) {
            fs::remove_dir_all(&entry_path).map_err(io_error)?;
        } else if file_type.is_file() {
            fs::remove_file(&entry_path).map_err(io_error)?;
        }
    }
    Ok(())
}

fn is_wol_owned_asset_directory(name: &std::ffi::OsStr) -> bool {
    let name = name.to_string_lossy().to_ascii_lowercase();
    name.ends_with(".sc2map") || name.ends_with(".sc2mod")
}

fn collect_regular_files(path: &Path) -> Result<Vec<PathBuf>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    if !path.is_dir() {
        return Err(format!("Campaign target {} is not a directory.", path.display()));
    }
    let mut files = Vec::new();
    collect_regular_files_inner(path, &mut files)?;
    Ok(files)
}

fn collect_regular_files_or_file(path: &Path) -> Result<Vec<PathBuf>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file_type = fs::symlink_metadata(path).map_err(io_error)?.file_type();
    if file_type.is_symlink() {
        return Err(format!("Refusing to back up symlink {}.", path.display()));
    }
    if file_type.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    if file_type.is_dir() {
        return collect_regular_files(path);
    }
    Err(format!("Dependency {} is neither a file nor directory.", path.display()))
}

fn collect_regular_files_inner(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(path).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let entry_path = entry.path();
        let file_type = entry.file_type().map_err(io_error)?;
        if file_type.is_symlink() {
            return Err(format!("Refusing to back up symlink {}.", entry_path.display()));
        }
        if file_type.is_dir() {
            collect_regular_files_inner(&entry_path, files)?;
        } else if file_type.is_file() {
            files.push(entry_path);
        }
    }
    Ok(())
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    let temporary = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    let json = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    let mut file = File::create(&temporary).map_err(io_error)?;
    file.write_all(&json).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    atomic_replace(&temporary, path)
}

#[cfg(not(windows))]
pub(crate) fn atomic_replace(temporary: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(temporary, destination).map_err(io_error)
}

#[cfg(windows)]
pub(crate) fn atomic_replace(temporary: &Path, destination: &Path) -> Result<(), String> {
    use std::iter;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};

    let temporary = temporary.as_os_str().encode_wide().chain(iter::once(0)).collect::<Vec<_>>();
    let destination = destination.as_os_str().encode_wide().chain(iter::once(0)).collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            temporary.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 { Err(io_error(io::Error::last_os_error())) } else { Ok(()) }
}

fn write_pending_install_journal(root: &Path, journal: &PendingInstallJournal) -> Result<(), String> {
    ensure_manager_root(root)?;
    write_json_atomic(&pending_install_path(root), journal)
}

fn read_pending_install_journal(root: &Path) -> Result<Option<PendingInstallJournal>, String> {
    validate_manager_root(root)?;
    let path = pending_install_path(root);
    if !path.is_file() {
        return Ok(None);
    }
    let json = fs::read_to_string(&path).map_err(io_error)?;
    let journal: PendingInstallJournal = serde_json::from_str(&json)
        .map_err(|_| "Pending install journal is corrupt; no files were changed.".to_string())?;
    if journal.format != 1 {
        return Err("Pending install journal has an unsupported format.".into());
    }
    validate_pending_install_journal(root, &journal)?;
    if let Some(snapshot) = &journal.previous_install {
        validate_managed_state(&snapshot.state)?;
    }
    if let Some(state) = &journal.new_state {
        validate_managed_state(state)?;
    }
    Ok(Some(journal))
}

fn validate_pending_install_journal(root: &Path, journal: &PendingInstallJournal) -> Result<(), String> {
    if !journal.profile_transaction.entries.is_empty() {
        if journal.profile_roots.is_empty() {
            return Err("Pending install journal has profile changes but no approved profile roots.".into());
        }
        for profile_root in &journal.profile_roots {
            if !safe_absolute_path(profile_root) || !approved_pending_profile_root(profile_root) {
                return Err("Pending install journal has a profile root outside SC2 or CCM's profile store.".into());
            }
            if let Ok(metadata) = fs::symlink_metadata(profile_root) {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err("Pending install journal references an unsafe profile root.".into());
                }
            }
        }
        let staging_root = manager_root(root).join("staging");
        for entry in &journal.profile_transaction.entries {
            let path_is_allowed = entry.path.is_absolute()
                && journal
                    .profile_roots
                    .iter()
                    .any(|profile_root| path_has_only_normal_suffix(&entry.path, profile_root));
            if !path_is_allowed {
                return Err("Pending install journal references a profile path outside the selected SC2 profile.".into());
            }
            if let Ok(metadata) = fs::symlink_metadata(&entry.path) {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err("Pending install journal references an unsafe profile file.".into());
                }
            }
            if let Some(backup) = &entry.backup {
                if !backup.is_absolute() || !path_has_only_normal_suffix(backup, &staging_root) {
                    return Err("Pending install journal has an invalid profile rollback backup path.".into());
                }
                let metadata = fs::symlink_metadata(backup)
                    .map_err(|_| "Pending install journal is missing a profile rollback backup.".to_string())?;
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err("Pending install journal profile rollback backup is unsafe.".into());
                }
            }
        }
    }
    if let Some(snapshot) = &journal.previous_install {
        let staging_root = manager_root(root).join("staging");
        if !snapshot.backup_snapshot.is_absolute()
            || !path_has_only_normal_suffix(&snapshot.backup_snapshot, &staging_root)
        {
            return Err("Pending install journal has an invalid previous-install backup path.".into());
        }
        let metadata = fs::symlink_metadata(&snapshot.backup_snapshot)
            .map_err(|_| "Pending install journal is missing the previous-install backup.".to_string())?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("Pending install journal previous-install backup is unsafe.".into());
        }
        for file in &snapshot.files {
            let _ = safe_relative_path(&file.destination)?;
            if !file.staged_path.is_absolute() || !path_has_only_normal_suffix(&file.staged_path, &staging_root) {
                return Err("Pending install journal has an invalid staged package path.".into());
            }
            let metadata = fs::symlink_metadata(&file.staged_path)
                .map_err(|_| "Pending install journal is missing a staged previous package file.".to_string())?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err("Pending install journal staged previous package file is unsafe.".into());
            }
        }
    }
    Ok(())
}

fn safe_absolute_path(path: &Path) -> bool {
    path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Prefix(_) | Component::RootDir | Component::Normal(_)))
}

fn approved_pending_profile_root(path: &Path) -> bool {
    if profile_store_base().is_ok_and(|base| path_has_only_normal_suffix(path, &base)) {
        return true;
    }
    // The selected live root must still have the shape we required at install
    // time.  This permits portable/test installations while rejecting a
    // journal that names an arbitrary ordinary directory as a profile root.
    is_starcraft_profile_directory(path)
}

fn path_has_only_normal_suffix(path: &Path, root: &Path) -> bool {
    path.strip_prefix(root)
        .ok()
        .is_some_and(|relative| relative.components().all(|component| matches!(component, Component::Normal(_))))
}

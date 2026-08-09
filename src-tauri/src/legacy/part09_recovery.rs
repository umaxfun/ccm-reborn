fn record_original_file(
    original: &Path,
    root: &Path,
    backup_root: &Path,
    backup_dir: &str,
    files: &mut Vec<ManagedFile>,
    positions: &mut HashMap<String, usize>,
) -> Result<(), String> {
    let destination = path_string(
        original
            .strip_prefix(root)
            .map_err(|_| "Campaign backup escaped game directory.")?,
    );
    if positions.contains_key(&destination) {
        return Ok(());
    }
    let backup = backup_root.join(&destination);
    copy_file(original, &backup)?;
    positions.insert(destination.clone(), files.len());
    files.push(ManagedFile {
        destination: destination.clone(),
        original_existed: true,
        backup_path: Some(format!("{backup_dir}/{destination}")),
        installed_sha256: None,
    });
    Ok(())
}

fn copy_regular_tree(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.is_dir() {
        return Err(format!("Expected directory {} is missing.", source.display()));
    }
    for file in collect_regular_files(source)? {
        let relative = file
            .strip_prefix(source)
            .map_err(|_| "Backup file escaped its root.")?;
        copy_file(&file, &destination.join(relative))?;
    }
    Ok(())
}

fn snapshot_previous_install(
    root: &Path,
    staging: &Path,
    target: &str,
) -> Result<Option<PreviousInstallSnapshot>, String> {
    let Some(state) = read_state_for_target(root, target)? else {
        return Ok(None);
    };
    let backup_root = manager_root(root).join(safe_backup_path(&state.backup_dir)?);
    let backup_snapshot = staging.join("previous-install-backup");
    if backup_root.is_dir() {
        copy_regular_tree(&backup_root, &backup_snapshot)?;
    } else if state.files.iter().any(|file| file.original_existed) {
        return Err(format!("Previous backup directory {} is missing; refusing update.", backup_root.display()));
    } else {
        fs::create_dir_all(&backup_snapshot).map_err(io_error)?;
    }
    let mut files = Vec::new();
    for managed in state.files.iter().filter(|file| file.installed_sha256.is_some()) {
        let destination = safe_relative_path(&managed.destination)?;
        let current = safe_game_path(root, &destination)?;
        if !current.is_file() {
            return Err(format!("Active campaign file {} is missing outside CCM Reborn; refusing update.", managed.destination));
        }
        let expected = managed.installed_sha256.as_deref().unwrap_or_default();
        if sha256_file(&current)? != expected {
            // Mods is intentionally a global repairable cache. Another slot
            // may have installed a different version of this dependency; it
            // is not user drift and must not make Repair unavailable.
            if managed.destination.starts_with("Mods/") {
                let staged_path = staging.join("previous-install-files").join(&destination);
                copy_file(&current, &staged_path)?;
                files.push(PreviousInstalledFile {
                    destination: managed.destination.clone(),
                    staged_path,
                });
                continue;
            }
            return Err(format!("Active campaign file {} was changed outside CCM Reborn; refusing update.", managed.destination));
        }
        let staged_path = staging.join("previous-install-files").join(&destination);
        copy_file(&current, &staged_path)?;
        files.push(PreviousInstalledFile {
            destination: managed.destination.clone(),
            staged_path,
        });
    }
    let manifest_path = installed_manifest_path(root, &state.target_path_or_first_clear())?;
    let manifest = if manifest_path.is_file() {
        Some(profile_core::read_installed_manifest(&manifest_path)?)
    } else {
        None
    };
    Ok(Some(PreviousInstallSnapshot {
        state,
        backup_snapshot,
        files,
        manifest,
    }))
}

fn restore_previous_install(root: &Path, snapshot: &PreviousInstallSnapshot) -> Result<(), String> {
    restore_previous_install_with_backup_mode(root, snapshot, false)
}

/// Recovery may run after the old package's original-file backup was only
/// partially retired.  In that case the exact staged copy is authoritative
/// and can safely rebuild CCM's own backup directory.
fn restore_previous_install_after_interruption(root: &Path, snapshot: &PreviousInstallSnapshot) -> Result<(), String> {
    restore_previous_install_with_backup_mode(root, snapshot, true)
}

fn restore_previous_install_with_backup_mode(
    root: &Path,
    snapshot: &PreviousInstallSnapshot,
    replace_existing_backup: bool,
) -> Result<(), String> {
    let backup_root = manager_root(root).join(safe_backup_path(&snapshot.state.backup_dir)?);
    if backup_root.exists() {
        if !replace_existing_backup {
            return Err(format!("Previous backup path {} unexpectedly exists.", backup_root.display()));
        }
        let metadata = fs::symlink_metadata(&backup_root).map_err(io_error)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(format!("Previous backup path {} is not a regular CCM backup directory.", backup_root.display()));
        }
        fs::remove_dir_all(&backup_root).map_err(io_error)?;
    }
    copy_regular_tree(&snapshot.backup_snapshot, &backup_root)?;
    for directory in &snapshot.state.cleared_directories {
        clear_campaign_target(root, directory)?;
    }
    let shared_roots = shared_dependency_roots(root, &snapshot.state)?;
    let restore_roots = dependency_roots_from_managed_files(&snapshot.state.files)
        .into_iter()
        .filter(|root| !shared_roots.contains(root))
        .collect::<Vec<_>>();
    clear_dependency_roots(root, &restore_roots)?;
    for file in &snapshot.files {
        if destination_is_under_roots(&file.destination, &shared_roots) {
            continue;
        }
        let destination = safe_game_path(root, &safe_relative_path(&file.destination)?)?;
        copy_file(&file.staged_path, &destination)?;
    }
    if let Some(manifest) = &snapshot.manifest {
        let manifest_path = installed_manifest_path(root, &snapshot.state.target_path_or_first_clear())?;
        write_installed_manifest_atomic(&manifest_path, manifest)?;
    }
    write_state_for_target(root, &snapshot.state)
}

fn rollback_install_failure(
    root: &Path,
    new_state: &ManagedState,
    new_manifest_path: &Path,
    previous: Option<&PreviousInstallSnapshot>,
    profile_transaction: &ProfileTransaction,
) -> Result<(), String> {
    let mut errors = Vec::new();
    if let Err(error) = restore_failed_install_exactly(root, new_state) {
        errors.push(format!("campaign rollback failed: {error}"));
    }
    let _ = fs::remove_file(new_manifest_path);
    let _ = remove_state_for_target(root, new_state);
    let _ = fs::remove_file(journal_path(root));
    let _ = fs::remove_dir_all(manager_root(root).join(&new_state.backup_dir));
    if let Some(previous) = previous {
        if let Err(error) = restore_previous_install(root, previous) {
            errors.push(format!("previous campaign restore failed: {error}"));
        }
    }
    if let Err(error) = profile_transaction.rollback() {
        errors.push(format!("profile rollback failed: {error}"));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn rollback_before_new_state(
    root: &Path,
    previous: Option<&PreviousInstallSnapshot>,
    profile_transaction: &ProfileTransaction,
) -> Result<(), String> {
    let mut errors = Vec::new();
    if let Some(previous) = previous {
        if let Err(error) = restore_previous_install(root, previous) {
            errors.push(format!("previous campaign restore failed: {error}"));
        }
    }
    if let Err(error) = profile_transaction.rollback() {
        errors.push(format!("profile rollback failed: {error}"));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn restore_existing_campaign(root: &Path, target: &str) -> Result<RestoreResult, String> {
    let Some(state) = read_state_for_target(root, target)? else {
        return Ok(RestoreResult {
            restored_files: 0,
            conflicts: Vec::new(),
        });
    };

    let mut conflicts = Vec::new();
    let shared_roots = shared_dependency_roots(root, &state)?;
    for file in &state.files {
        if destination_is_under_roots(&file.destination, &shared_roots) {
            continue;
        }
            let destination = safe_game_path(root, &safe_relative_path(&file.destination)?)?;
            if !destination.is_file() {
                // A pre-existing file may legitimately be absent after the
                // package replaced the whole branch; force_restore will put
                // it back from the snapshot. A package-owned file, however,
                // must still exist exactly where the manifest says it does.
                if file.installed_sha256.is_some() {
                    conflicts.push(file.destination.clone());
                }
                continue;
            }
            let expected_hash = file
                .installed_sha256
                .clone()
                .or_else(|| {
                    file.backup_path.as_deref().and_then(|backup| {
                        safe_backup_path(backup)
                            .ok()
                            .and_then(|path| sha256_file(&manager_root(root).join(path)).ok())
                    })
                });
            match (expected_hash, sha256_file(&destination)) {
                (Some(expected), Ok(actual)) if actual == expected => {}
                _ => conflicts.push(file.destination.clone()),
            }
    }
    if !conflicts.is_empty() {
        return Ok(RestoreResult {
            restored_files: 0,
            conflicts,
        });
    }

    archive_installed_manifest(root, &state)?;
    force_restore(root, &state)?;
    remove_state_for_target(root, &state)?;
    retire_unused_shared_baselines(root, &dependency_roots_from_managed_files(&state.files))?;
    let _ = fs::remove_dir_all(manager_root(root).join(&state.backup_dir));
    if let Ok(path) = installed_manifest_path(root, &state.target_path_or_first_clear()) {
        let _ = fs::remove_file(path);
    }
    Ok(RestoreResult {
        restored_files: state.files.len(),
        conflicts: Vec::new(),
    })
}

fn archive_installed_manifest(root: &Path, state: &ManagedState) -> Result<(), String> {
    let active_path = installed_manifest_path(root, &state.target_path_or_first_clear())?;
    if !active_path.is_file() {
        return Ok(());
    }
    let manifest = profile_core::read_installed_manifest(&active_path)?;
    let history_path = manager_root(root)
        .join("installed")
        .join("history")
        .join(format!("{}-{}.json", unix_timestamp(), Uuid::new_v4()));
    write_installed_manifest_atomic(&history_path, &manifest)
}

fn shared_dependency_roots(root: &Path, state: &ManagedState) -> Result<HashSet<PathBuf>, String> {
    let target = state.target_path_or_first_clear();
    let other_roots = read_managed_states(root)?
        .into_iter()
        .filter(|candidate| candidate.target_path_or_first_clear() != target)
        .flat_map(|candidate| dependency_roots_from_managed_files(&candidate.files))
        .map(|path| path_string(&path).to_ascii_lowercase())
        .collect::<HashSet<_>>();
    Ok(dependency_roots_from_managed_files(&state.files)
        .into_iter()
        .filter(|path| other_roots.contains(&path_string(path).to_ascii_lowercase()))
        .collect())
}

fn destination_is_under_roots(destination: &str, roots: &HashSet<PathBuf>) -> bool {
    dependency_root_from_destination(Path::new(destination))
        .is_some_and(|root| roots.contains(&root))
}

fn force_restore(root: &Path, state: &ManagedState) -> Result<(), String> {
    for directory in &state.cleared_directories {
        clear_campaign_target(root, directory)?;
    }
    let shared_roots = shared_dependency_roots(root, state)?;
    let mut baseline_roots = HashSet::new();
    for dependency_root in dependency_roots_from_managed_files(&state.files) {
        if !shared_roots.contains(&dependency_root) && restore_shared_baseline(root, &dependency_root)? {
            baseline_roots.insert(dependency_root);
        }
    }
    let restore_roots = dependency_roots_from_managed_files(&state.files)
        .into_iter()
        .filter(|root| !shared_roots.contains(root) && !baseline_roots.contains(root))
        .collect::<Vec<_>>();
    clear_dependency_roots(root, &restore_roots)?;
    for file in &state.files {
        if destination_is_under_roots(&file.destination, &shared_roots)
            || destination_is_under_roots(&file.destination, &baseline_roots)
        {
            continue;
        }
        let destination = safe_game_path(root, &safe_relative_path(&file.destination)?)?;
        if file.original_existed {
            let backup_path = file
                .backup_path
                .as_deref()
                .ok_or("A backup path is missing from the managed state.")?;
            let backup = manager_root(root).join(safe_backup_path(backup_path)?);
            if !backup.is_file() {
                return Err(format!("Backup for {} is missing; refusing to continue.", file.destination));
            }
            copy_file(&backup, &destination)?;
        } else if destination.is_file() {
            fs::remove_file(&destination).map_err(io_error)?;
        }
    }
    Ok(())
}

/// A state file alone is not proof that its package still owns the live game
/// files: a process can stop halfway through `force_restore`.  Recovery uses
/// these hashes before deciding that an old install can be left in place.
fn managed_install_files_match(root: &Path, state: &ManagedState) -> Result<bool, String> {
    let mut package_files = 0usize;
    for file in state.files.iter().filter(|file| file.installed_sha256.is_some()) {
        package_files += 1;
        let destination = safe_game_path(root, &safe_relative_path(&file.destination)?)?;
        let metadata = match fs::symlink_metadata(&destination) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(io_error(error)),
        };
        if metadata.file_type().is_symlink() {
            return Err(format!("Managed package file {} is a symlink; refusing recovery.", destination.display()));
        }
        if !metadata.is_file() {
            return Ok(false);
        }
        let expected = file.installed_sha256.as_deref().unwrap_or_default();
        if sha256_file(&destination)? != expected {
            return Ok(false);
        }
    }
    Ok(package_files > 0)
}

fn recover_interrupted_install(root: &Path) -> Result<bool, String> {
    if let Some(pending) = read_pending_install_journal(root)? {
        let pending_path = pending_install_path(root);
        // A process can die after committing state but before deleting the
        // intent record.  That is a completed install, not a reason to undo
        // the player's newly selected campaign.
        let recovery_target = pending
            .new_state
            .as_ref()
            .map(ManagedState::target_path_or_first_clear)
            .or_else(|| pending.previous_install.as_ref().map(|previous| previous.state.target_path_or_first_clear()));
        let active_state = match recovery_target.as_deref() {
            Some(target) => read_state_for_target(root, target)?,
            None => None,
        };
        let state_matches_completed_install = pending.new_state.as_ref().is_some_and(|new_state| {
            active_state
                .as_ref()
                .is_some_and(|active| active.backup_dir == new_state.backup_dir)
        });
        let previous_is_still_active = if let (Some(previous), Some(active)) = (&pending.previous_install, &active_state) {
            active.backup_dir == previous.state.backup_dir && managed_install_files_match(root, active)?
        } else {
            false
        };
        if pending.completed || state_matches_completed_install {
            let _ = fs::remove_file(journal_path(root));
            let _ = fs::remove_file(&pending_path);
            return Ok(false);
        }

        let mut errors = Vec::new();
        if let Some(new_state) = &pending.new_state {
            if let Err(error) = restore_failed_install_exactly(root, new_state) {
                errors.push(format!("new campaign rollback failed: {error}"));
            }
            if let Ok(manifest_path) = installed_manifest_path(root, &new_state.target_path_or_first_clear()) {
                let _ = fs::remove_file(manifest_path);
            }
            let _ = remove_state_for_target(root, new_state);
            let _ = fs::remove_dir_all(manager_root(root).join(&new_state.backup_dir));
        }
        if let Some(previous) = &pending.previous_install {
            // A crash immediately after the profile switch happens before
            // `restore_existing_campaign` retires the old state.  Its game
            // files are still correct; only the profile needs rollback.
            if previous_is_still_active {
                // no-op
            } else if let Err(error) = restore_previous_install_after_interruption(root, previous) {
                errors.push(format!("previous campaign restore failed: {error}"));
            }
        }
        if let Err(error) = pending.profile_transaction.rollback() {
            errors.push(format!("profile rollback failed: {error}"));
        }
        if !errors.is_empty() {
            return Err(format!(
                "Interrupted install recovery could not finish; no further files were changed: {}",
                errors.join("; ")
            ));
        }
        let _ = fs::remove_file(journal_path(root));
        let _ = fs::remove_file(pending_path);
        return Ok(true);
    }

    // Legacy installs have only the game-directory journal and therefore can
    // restore only their own original files.  New installs always use the
    // profile-aware pending journal above.
    let path = journal_path(root);
    if !path.is_file() {
        return Ok(false);
    }
    let transaction = read_state_file(&path)?;
    let transaction_target = transaction.target_path_or_first_clear();
    if let Some(state) = read_state_for_target(root, &transaction_target)? {
        if state.backup_dir == transaction.backup_dir {
            let _ = fs::remove_file(path);
            return Ok(false);
        }
    }
    force_restore(root, &transaction)?;
    if let Ok(path) = installed_manifest_path(root, &transaction.target_path_or_first_clear()) {
        let _ = fs::remove_file(path);
    }
    let _ = fs::remove_file(path);
    Ok(true)
}

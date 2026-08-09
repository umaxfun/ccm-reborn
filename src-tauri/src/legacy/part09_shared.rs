fn shared_baseline_path(root: &Path, dependency_root: &Path) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(path_string(dependency_root).to_ascii_lowercase().as_bytes());
    manager_root(root).join("shared-dependencies").join(format!("{}.json", hex::encode(hasher.finalize())))
}

fn read_shared_baseline(root: &Path, dependency_root: &Path) -> Result<Option<SharedDependencyBaseline>, String> {
    let path = shared_baseline_path(root, dependency_root);
    if !path.is_file() { return Ok(None); }
    let baseline = serde_json::from_str::<SharedDependencyBaseline>(&fs::read_to_string(path).map_err(io_error)?)
        .map_err(|_| "Shared dependency baseline is corrupt; no files were changed.".to_string())?;
    if baseline.format != 1 || safe_relative_path(&baseline.dependency_root)? != dependency_root {
        return Err("Shared dependency baseline does not match its dependency root.".into());
    }
    Ok(Some(baseline))
}

fn ensure_shared_dependency_baselines(root: &Path, roots: &[PathBuf]) -> Result<(), String> {
    ensure_manager_root(root)?;
    let active = read_managed_states(root)?;
    for dependency_root in roots {
        if read_shared_baseline(root, dependency_root)?.is_some() { continue; }
        if active.iter().any(|state| dependency_roots_from_managed_files(&state.files).contains(dependency_root)) { continue; }
        let source = safe_game_path(root, dependency_root)?;
        let backup_dir = format!("shared-dependencies/{}", Uuid::new_v4());
        let backup = manager_root(root).join(&backup_dir).join("content");
        let (original_existed, original_was_file) = if source.exists() {
            let kind = fs::symlink_metadata(&source).map_err(io_error)?.file_type();
            if kind.is_symlink() { return Err(format!("Refusing to baseline symlinked dependency {}.", source.display())); }
            if kind.is_file() { copy_file(&source, &backup)?; (true, true) }
            else if kind.is_dir() { copy_regular_tree(&source, &backup)?; (true, false) }
            else { return Err(format!("Dependency {} is neither file nor directory.", source.display())); }
        } else { (false, false) };
        let baseline = SharedDependencyBaseline { format: 1, dependency_root: path_string(dependency_root), original_existed, original_was_file, backup_dir };
        write_json_atomic(&shared_baseline_path(root, dependency_root), &baseline)?;
    }
    Ok(())
}

fn restore_shared_baseline(root: &Path, dependency_root: &Path) -> Result<bool, String> {
    let Some(baseline) = read_shared_baseline(root, dependency_root)? else { return Ok(false); };
    let destination = safe_game_path(root, dependency_root)?;
    if destination.exists() {
        let kind = fs::symlink_metadata(&destination).map_err(io_error)?.file_type();
        if kind.is_symlink() { return Err(format!("Refusing to restore symlinked dependency {}.", destination.display())); }
        if kind.is_dir() { fs::remove_dir_all(&destination).map_err(io_error)?; } else if kind.is_file() { fs::remove_file(&destination).map_err(io_error)?; }
        else { return Err(format!("Dependency {} is neither file nor directory.", destination.display())); }
    }
    if baseline.original_existed {
        let backup = manager_root(root).join(safe_relative_path(&baseline.backup_dir)?).join("content");
        if baseline.original_was_file { copy_file(&backup, &destination)?; } else { copy_regular_tree(&backup, &destination)?; }
    }
    Ok(true)
}

fn retire_unused_shared_baselines(root: &Path, roots: &[PathBuf]) -> Result<(), String> {
    let active = read_managed_states(root)?;
    for dependency_root in roots {
        if active.iter().any(|state| dependency_roots_from_managed_files(&state.files).contains(dependency_root)) { continue; }
        let Some(baseline) = read_shared_baseline(root, dependency_root)? else { continue; };
        let _ = fs::remove_file(shared_baseline_path(root, dependency_root));
        let _ = fs::remove_dir_all(manager_root(root).join(safe_relative_path(&baseline.backup_dir)?));
    }
    Ok(())
}

/// Rollback is different from a user-facing restore: it must return the game
/// to the bytes that existed immediately before this transaction, even when a
/// different slot also references the same global Mods cache.
fn restore_failed_install_exactly(root: &Path, state: &ManagedState) -> Result<(), String> {
    for directory in &state.cleared_directories { clear_campaign_target(root, directory)?; }
    clear_dependency_roots(root, &dependency_roots_from_managed_files(&state.files))?;
    for file in &state.files {
        let destination = safe_game_path(root, &safe_relative_path(&file.destination)?)?;
        if file.original_existed {
            let backup = manager_root(root).join(safe_backup_path(file.backup_path.as_deref().ok_or("A backup path is missing from the managed state.")?)?);
            if !backup.is_file() { return Err(format!("Backup for {} is missing; refusing to continue.", file.destination)); }
            copy_file(&backup, &destination)?;
        } else if destination.is_file() {
            fs::remove_file(destination).map_err(io_error)?;
        }
    }
    Ok(())
}

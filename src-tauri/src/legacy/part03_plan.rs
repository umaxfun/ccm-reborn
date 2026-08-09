#[tauri::command]
async fn plan_campaign_install(request: InstallRequest) -> Result<DryRunPlan, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = require_desktop_game_root(&request.game_dir)?;
        let mut request = request;
        request.game_dir = root.display().to_string();
        plan_campaign_install_blocking(request)
    })
        .await
        .map_err(|error| format!("Dry-run worker failed: {error}"))?
}

fn plan_campaign_install_blocking(request: InstallRequest) -> Result<DryRunPlan, String> {
    validate_campaign_id(&request.campaign_id)?;
    if request.title.trim().is_empty() {
        return Err("Campaign title is required.".into());
    }

    let root = PathBuf::from(request.game_dir.trim());
    if !root.is_dir() {
        return Err("Choose an existing StarCraft II directory before planning an install.".into());
    }
    validate_manager_root(&root)?;
    if pending_install_path(&root).is_file() {
        return Err("An interrupted CCM operation needs recovery before a new dry-run can be reviewed.".into());
    }
    if looks_like_starcraft(&root)
        && request.profile_dir.as_deref().map(str::trim).filter(|path| !path.is_empty()).is_none()
    {
        return Err("Choose the exact StarCraft II account profile before reviewing a live install.".into());
    }
    let archive = acquire_archive(&request.archive_source)?;
    let archive_path = &archive.path;

    let archive_size = fs::metadata(&archive_path).map_err(io_error)?.len();
    validate_declared_package_size(request.package_size, archive_size)?;
    if archive_size > MAX_ARCHIVE_BYTES {
        return Err("Package is larger than the allowed archive size.".into());
    }
    let archive_sha256 = sha256_file(&archive_path)?;
    let expected_sha256 = normalize_sha256(&request.sha256)?;
    if archive_sha256 != expected_sha256 {
        return Err("The local archive does not match the catalog SHA-256. No files were changed.".into());
    }

    let package = read_ccm_package(&archive_path)?;
    let previous_state = read_state_for_target(&root, &package.target_path)?;
    let package_files = inspect_ccm_package_files(&archive_path, &package)?;
    refuse_nested_wol_package(&package.target_path, &package_files)?;
    let campaign_files = collect_campaign_target_files(&root, &package.target_path)?;
    let dependency_roots = dependency_roots_from_planned_files(&package_files);
    let mut dependency_files = Vec::new();
    for dependency_root in &dependency_roots {
        dependency_files.extend(collect_regular_files_or_file(&root.join(dependency_root))?);
    }
    let current_managed = current_managed_campaign(&root, &package.target_path)?;
    let current_dependencies = current_managed
        .as_ref()
        .map(|campaign| campaign.dependencies.clone())
        .unwrap_or_default();
    let profile_campaign_id = current_managed
        .as_ref()
        .map(|campaign| campaign.campaign_id.as_str())
        .unwrap_or("");
    let target_manifest_path = installed_manifest_path(&root, &package.target_path)?;
    if previous_state.is_none() && target_manifest_path.is_file() {
        let _ = profile_core::read_installed_manifest(&target_manifest_path)?;
        return Err(format!(
            "Installed manifest {} has no active managed state; refusing to plan a destructive update. Run recovery or restore first.",
            target_manifest_path.display()
        ));
    }
    let previous_manifest_path = previous_state
        .as_ref()
        .map(|state| installed_manifest_path(&root, &state.target_path_or_first_clear()))
        .transpose()?;
    let previous_manifest = previous_manifest_path
        .as_ref()
        .filter(|path| path.is_file())
        .map(|path| profile_core::read_installed_manifest(path))
        .transpose()?;
    let previous_install_files = previous_state
        .as_ref()
        .map(|state| state.files.iter().filter(|file| file.installed_sha256.is_some()).count())
        .unwrap_or(0);
    let update_kind = if previous_state.is_some() {
        "update-existing-install"
    } else {
        "fresh-install"
    };
    let operation_id = Uuid::new_v4().to_string();
    let profile = plan_profile_transition(
        &root,
        &package.target_path,
        profile_campaign_id,
        &request.campaign_id,
        current_managed.as_ref().map(|campaign| campaign.title.as_str()),
        &current_dependencies,
        request.profile_dir.as_deref(),
    )?;
    let backup_root = manager_root(&root)
        .join("backups")
        .join(format!("{}-{operation_id}", request.campaign_id));
    let mut file_changes = Vec::with_capacity(
        previous_install_files + campaign_files.len() + dependency_files.len() + package_files.len(),
    );
    if let Some(state) = &previous_state {
        // State records both package-owned files and original files that the
        // package replaced.  Show every one: a dry-run is a review surface,
        // not a terse progress indicator.
        for file in &state.files {
            let destination = root.join(safe_relative_path(&file.destination)?);
            let (source, size, sha256) = if file.original_existed {
                let backup = file
                    .backup_path
                    .as_deref()
                    .ok_or("A previous original file has no backup path.")?;
                let source = manager_root(&root).join(safe_backup_path(backup)?);
                let size = fs::metadata(&source).map(|metadata| metadata.len()).unwrap_or(0);
                let sha256 = sha256_file(&source).ok();
                (source, size, sha256)
            } else {
                let size = fs::metadata(&destination).map(|metadata| metadata.len()).unwrap_or(0);
                (destination.clone(), size, file.installed_sha256.clone())
            };
            let operation = if file.original_existed {
                "restore previous original before update"
            } else {
                "remove previous package file before update"
            };
            file_changes.push(FileChangePlan {
                source: source.display().to_string(),
                destination: destination.display().to_string(),
                operation: operation.into(),
                kind: "previous managed install".into(),
                size,
                sha256,
                detail: Some(if file.original_existed {
                    "restored from the previous installation's original-file backup".into()
                } else {
                    "verified against the previous managed state before changing it".into()
                }),
            });
        }
    }
    for file in campaign_files.iter().chain(dependency_files.iter()) {
        let relative = file
            .strip_prefix(&root)
            .map_err(|_| "Planned file escaped the game directory.")?;
        let metadata = fs::metadata(file).map_err(io_error)?;
        file_changes.push(FileChangePlan {
            source: file.display().to_string(),
            destination: backup_root.join(relative).display().to_string(),
            operation: "snapshot before clear/replace".into(),
            kind: if relative.starts_with("Mods") { "existing dependency".into() } else { "existing campaign file".into() },
            size: metadata.len(),
            sha256: Some(sha256_file(file)?),
            detail: None,
        });
    }
    for file in &package_files {
        file_changes.push(FileChangePlan {
            source: format!("{} :: {}", archive_path.display(), file.source),
            destination: root.join(&file.destination).display().to_string(),
            operation: "install package file".into(),
            kind: if file.destination.starts_with("Mods/") { "package dependency".into() } else { "package campaign file".into() },
            size: file.size,
            sha256: None,
            detail: Some(format!("archive sha256 {}", archive_sha256)),
        });
    }

    let package_bytes = package_files.iter().map(|file| file.size).sum();
    let campaign_bytes_to_clear = campaign_files
        .iter()
        .filter_map(|file| fs::metadata(file).ok().map(|metadata| metadata.len()))
        .sum();
    let profile_files_to_snapshot = profile.files.iter().filter(|file| file.action.starts_with("snapshot")).count();
    let profile_bytes_to_snapshot = profile
        .files
        .iter()
        .filter(|file| file.action.starts_with("snapshot"))
        .map(|file| file.size)
        .sum();
    let profile_files_to_restore = profile.files.iter().filter(|file| file.action.starts_with("restore")).count();
    let profile_bytes_to_restore = profile
        .files
        .iter()
        .filter(|file| file.action.starts_with("restore"))
        .map(|file| file.size)
        .sum();
    let progress_updates = profile
        .files
        .iter()
        .filter(|file| file.kind == "campaign-progress")
        .count();
    let mut warnings = vec![
        "Dry-run only: no campaign, save, bank, or dependency files were changed.".into(),
        "Applying this plan later requires StarCraft II to be fully closed.".into(),
    ];
    if previous_state.is_some() {
        warnings.push(format!(
            "Previous managed install found: {previous_install_files} package files will be restored or removed before the new package is copied."
        ));
    }
    if previous_state.is_some() && previous_manifest.is_none() {
        warnings.push("Previous install uses the legacy managed state; the next successful install will write an exact installed-manifest.json.".into());
    }
    warnings.extend(profile.warnings);
    if !dependency_roots.is_empty() {
        warnings.push("Package dependencies will be placed under the game Mods directory.".into());
    }
    Ok(DryRunPlan {
        operation_id,
        campaign_id: request.campaign_id,
        title: request.title,
        game_directory: root.display().to_string(),
        target_path: package.target_path,
        archive_size,
        archive_sha256,
        update_kind: update_kind.into(),
        previous_install_manifest: previous_manifest_path.map(|path| path.display().to_string()),
        previous_install_campaign_id: previous_manifest.as_ref().map(|manifest| manifest.campaign_id.clone()).or_else(|| previous_state.as_ref().map(|state| state.campaign_id.clone())),
        previous_install_version: previous_manifest.as_ref().map(|manifest| manifest.version.clone()).or_else(|| previous_state.as_ref().map(|state| state.version.clone()).filter(|version| !version.is_empty())),
        previous_install_sha256: previous_manifest.as_ref().map(|manifest| manifest.package_sha256.clone()),
        previous_install_files,
        package_files: package_files.len(),
        package_bytes,
        campaign_files_to_clear: campaign_files.len(),
        campaign_bytes_to_clear,
        dependency_roots: dependency_roots.into_iter().map(|path| path_string(&path)).collect(),
        dependency_files_to_replace: dependency_files.len(),
        files_to_backup: campaign_files.len() + dependency_files.len(),
        profile_path: profile.profile_path,
        profile_store_path: profile.profile_store_path,
        profile_files_to_snapshot,
        profile_bytes_to_snapshot,
        profile_files_to_restore,
        profile_bytes_to_restore,
        progress_updates,
        progress_files: profile.files,
        progress_keys: profile.progress_keys,
        bank_plans: profile.bank_plans,
        file_changes,
        warnings,
    })
}

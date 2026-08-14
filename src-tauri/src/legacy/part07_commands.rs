#[tauri::command]
fn resolve_game_directory(path: String) -> Result<GameDirectoryCandidate, String> {
    let selected = PathBuf::from(path.trim());
    let resolved = find_game_root(&selected)
        .ok_or("StarCraft II was not found in that folder. Choose the game directory or StarCraft II.app.")?;
    Ok(game_directory_candidate(resolved))
}

#[tauri::command]
fn detect_game_directories() -> Result<Vec<GameDirectoryCandidate>, String> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    for location in standard_game_locations() {
        if let Some(root) = find_game_root(&location) {
            let canonical = root.canonicalize().unwrap_or(root);
            if seen.insert(canonical.clone()) {
                found.push(game_directory_candidate(canonical));
            }
        }
    }
    Ok(found)
}

#[tauri::command]
fn detect_starcraft_profiles() -> Result<Vec<StarcraftProfileCandidate>, String> {
    discover_starcraft_profiles().map(|profiles| {
        profiles
            .into_iter()
            .map(|path| StarcraftProfileCandidate {
                label: display_profile_path(&path),
                path: path.display().to_string(),
            })
            .collect()
    })
}

#[tauri::command]
async fn load_catalog(source: String) -> Result<LoadedCatalog, String> {
    tauri::async_runtime::spawn_blocking(move || load_catalog_blocking(source))
        .await
        .map_err(|error| format!("Catalog worker failed: {error}"))?
}

fn load_catalog_blocking(source: String) -> Result<LoadedCatalog, String> {
    let source = source.trim();
    if source.is_empty() {
        return Err("Enter a catalog.json path or an HTTPS catalog URL.".into());
    }

    let (catalog_json, source_kind, local_directory, fetched_remote) = if source.starts_with("https://") {
        match download_text(source, 2 * 1024 * 1024) {
            Ok(catalog) => (catalog, "remote".to_string(), None, true),
            Err(download_error) => (
                read_cached_catalog(source, 2 * 1024 * 1024)
                    .map_err(|cache_error| format!("{download_error} {cache_error}"))?,
                "cached".to_string(),
                None,
                false,
            ),
        }
    } else {
        let path = PathBuf::from(source);
        if path.file_name().and_then(|name| name.to_str()) != Some("catalog.json") {
            return Err("Local development catalogs must be named catalog.json.".into());
        }
        let canonical = path
            .canonicalize()
            .map_err(|_| "Local catalog.json was not found.".to_string())?;
        let directory = canonical
            .parent()
            .ok_or("Local catalog has no parent directory.")?
            .to_path_buf();
        (
            fs::read_to_string(&canonical).map_err(io_error)?,
            "local".to_string(),
            Some(directory),
            false,
        )
    };

    let catalog: CatalogDocument = serde_json::from_str(&catalog_json)
        .map_err(|_| "catalog.json is not valid JSON.".to_string())?;
    if catalog.format != 1 {
        return Err("This catalog uses an unsupported format.".into());
    }
    if catalog.name.trim().is_empty() {
        return Err("Catalog must have a name.".into());
    }

    let mut campaigns = Vec::with_capacity(catalog.campaigns.len());
    let mut ids = HashSet::new();
    for campaign in catalog.campaigns {
        validate_catalog_campaign(&campaign)?;
        if !ids.insert(campaign.id.clone()) {
            return Err(format!("Catalog contains campaign ID {} more than once.", campaign.id));
        }
        let package_source = match &local_directory {
            Some(directory) => resolve_local_package(directory, campaign.package.path.as_deref())?,
            None => campaign
                .package
                .url
                .as_deref()
                .filter(|url| url.starts_with("https://"))
                .map(str::to_owned)
                .ok_or_else(|| format!("Remote campaign {} needs an HTTPS package.url.", campaign.id))?,
        };
        campaigns.push(ResolvedCatalogCampaign {
            id: campaign.id,
            title: campaign.title,
            author: campaign.author,
            version: campaign.version,
            description: campaign.description,
            tags: campaign.tags,
            requirements: campaign.requirements,
            package: ResolvedCatalogPackage {
                source: package_source,
                sha256: normalize_sha256(&campaign.package.sha256)?,
                size: campaign.package.size,
            },
        });
    }

    if fetched_remote {
        // A cache is only written after the full catalog has parsed and each
        // entry passed its HTTPS, hash, size, and ID validation.
        let _ = cache_catalog(source, &catalog_json);
    }
    Ok(LoadedCatalog {
        format: catalog.format,
        name: catalog.name,
        updated_at: catalog.updated_at,
        source_kind,
        campaigns,
    })
}

#[tauri::command]
async fn install_campaign(window: tauri::Window, request: InstallRequest) -> Result<InstallResult, String> {
    let reporter = ProgressReporter::new(window);
    tauri::async_runtime::spawn_blocking(move || {
        let root = require_desktop_game_root(&request.game_dir)?;
        let mut request = request;
        request.game_dir = root.display().to_string();
        install_campaign_with_progress(request, Some(&reporter))
    })
        .await
        .map_err(|error| format!("Installation worker failed: {error}"))?
}

fn install_campaign_blocking(request: InstallRequest) -> Result<InstallResult, String> {
    install_campaign_with_progress(request, None)
}

fn install_campaign_with_progress(
    request: InstallRequest,
    reporter: Option<&ProgressReporter>,
) -> Result<InstallResult, String> {
    reporter.map(|reporter| reporter.status("preparing-install", "Preparing installation…"));
    validate_campaign_id(&request.campaign_id)?;
    if request.title.trim().is_empty() {
        return Err("Campaign title is required.".into());
    }

    let root = PathBuf::from(request.game_dir.trim());
    if !root.is_dir() {
        return Err("Choose an existing StarCraft II directory before installing.".into());
    }
    if looks_like_starcraft(&root) && request.profile_dir.as_deref().map(str::trim).filter(|path| !path.is_empty()).is_none() {
        return Err("Choose the exact StarCraft II account profile with --profile-dir before applying a live install.".into());
    }
    let _operation_lock = acquire_operation_lock(&root)?;
    recover_interrupted_install(&root)?;

    let expected_hash = normalize_sha256(&request.sha256)?;
    let archive = match acquire_archive(&request.archive_source, &expected_hash, request.package_size, reporter) {
        Ok(archive) => archive,
        Err(error) => {
            reporter.map(|reporter| reporter.failed("download-failed", &error));
            return Err(error);
        }
    };
    validate_declared_package_size(request.package_size, fs::metadata(&archive.path).map_err(io_error)?.len())?;
    // `install_archive` validates and stages the new package before it touches
    // the old install. This prevents a bad download from deleting the current
    // campaign first.
    reporter.map(|reporter| reporter.status("staging", "Verifying and staging package files…"));
    let result = install_archive(&root, &request, &archive.path, &expected_hash, reporter);
    if archive.is_temporary {
        let _ = fs::remove_file(&archive.path);
    }
    if let Err(error) = &result {
        reporter.map(|reporter| reporter.failed("install-failed", error));
    }
    result
}

#[tauri::command]
async fn restore_original_campaigns(game_dir: String, profile_dir: Option<String>, target_path: String) -> Result<RestoreResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = require_desktop_game_root(&game_dir)?;
        restore_original_campaigns_blocking(root.display().to_string(), profile_dir, target_path)
    })
        .await
        .map_err(|error| format!("Restore worker failed: {error}"))?
}

fn restore_original_campaigns_blocking(game_dir: String, profile_dir: Option<String>, target_path: String) -> Result<RestoreResult, String> {
    let root = PathBuf::from(game_dir.trim());
    if !root.is_dir() {
        return Err("Choose an existing StarCraft II directory first.".into());
    }
    if looks_like_starcraft(&root) && profile_dir.as_deref().map(str::trim).filter(|path| !path.is_empty()).is_none() {
        return Err("Choose the exact StarCraft II account profile with --profile-dir before restoring a live campaign.".into());
    }
    let _operation_lock = acquire_operation_lock(&root)?;
    recover_interrupted_install(&root)?;
    let target = path_string(&safe_campaign_target(&target_path)?);
    let Some(state) = read_state_for_target(&root, &target)? else {
        return Ok(RestoreResult {
            restored_files: 0,
            conflicts: Vec::new(),
        });
    };
    let staging = manager_root(&root)
        .join("staging")
        .join(format!("restore-{}", Uuid::new_v4()));
    fs::create_dir_all(&staging).map_err(io_error)?;
    let result = (|| {
        // Restoring originals retires the active managed state, so keep a
        // private copy in the same recovery journal used for mod-to-mod
        // switches.  If the app stops mid-restore, recovery returns the
        // player to the exact campaign/profile they had before clicking it.
        let source_target = state.target_path_or_first_clear();
        let previous_install = snapshot_previous_install(&root, &staging, &source_target)?;
        let source_dependencies = dependency_roots_from_managed_files(&state.files);
        let vanilla_profile = format!("vanilla-{}", campaign_slot_name(&source_target)?);
        let profile_transaction = apply_profile_transition_with_hook(
            &root,
            &source_target,
            &source_target,
            &state.campaign_id,
            &source_dependencies,
            &vanilla_profile,
            &[],
            &staging,
            profile_dir.as_deref(),
            |transaction, profile_roots| {
                write_pending_install_journal(
                    &root,
                    &PendingInstallJournal {
                        format: 1,
                        profile_transaction: transaction.clone(),
                        profile_roots: profile_roots.to_vec(),
                        previous_install: previous_install.clone(),
                        new_state: None,
                        completed: false,
                    },
                )
            },
        )?;
        let profile_roots = read_pending_install_journal(&root)?
            .map(|journal| journal.profile_roots)
            .unwrap_or_default();
        write_pending_install_journal(
            &root,
            &PendingInstallJournal {
                format: 1,
                profile_transaction: profile_transaction.clone(),
                profile_roots: profile_roots.clone(),
                previous_install: previous_install.clone(),
                new_state: None,
                completed: false,
            },
        )?;
        let restored = match restore_existing_campaign(&root, &source_target) {
            Ok(restored) => restored,
            Err(error) => {
                if profile_transaction.rollback().is_ok() {
                    let _ = fs::remove_file(pending_install_path(&root));
                }
                return Err(error);
            }
        };
        if !restored.conflicts.is_empty() {
            if profile_transaction.rollback().is_ok() {
                let _ = fs::remove_file(pending_install_path(&root));
            }
        } else {
            write_pending_install_journal(
                &root,
                &PendingInstallJournal {
                    format: 1,
                    profile_transaction,
                    profile_roots,
                    previous_install,
                    new_state: None,
                    completed: true,
                },
            )?;
            let _ = fs::remove_file(pending_install_path(&root));
        }
        Ok(restored)
    })();
    if !pending_install_path(&root).exists() {
        let _ = fs::remove_dir_all(staging);
    }
    result
}

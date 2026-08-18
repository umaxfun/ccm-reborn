fn install_archive(
    root: &Path,
    request: &InstallRequest,
    archive_path: &Path,
    expected_hash: &str,
    reporter: Option<&ProgressReporter>,
) -> Result<InstallResult, String> {
    ensure_manager_root(root)?;
    reporter.map(|reporter| reporter.status("verifying-package", "Verifying package integrity…"));
    let actual_hash = sha256_file(archive_path)?;
    if actual_hash != expected_hash {
        return Err("The downloaded archive does not match the catalog SHA-256. Installation stopped.".into());
    }
    let package = read_ccm_package(archive_path)?;
    let package_files = inspect_ccm_package_files(archive_path, &package)?;
    refuse_nested_wol_package(&package.target_path, &package_files)?;
    let manifest_path = installed_manifest_path(root, &package.target_path)?;
    let existing_state = read_state_for_target(root, &package.target_path)?;
    if existing_state.is_none() && manifest_path.is_file() {
        // A manifest without the active journal/state is ambiguous (for
        // example after a manually interrupted copy). Never clear that slot
        // automatically; ask recovery to establish a consistent state first.
        let _ = profile_core::read_installed_manifest(&manifest_path)?;
        return Err(format!(
            "Installed manifest {} has no active managed state; refusing to delete files. Run recovery or restore first.",
            manifest_path.display()
        ));
    }
    let managed_root = manager_root(root);
    let staging = managed_root.join("staging").join(Uuid::new_v4().to_string());
    fs::create_dir_all(&staging).map_err(io_error)?;
    let result = (|| {
        reporter.map(|reporter| reporter.status("extracting", "Extracting package into a safe staging area…"));
        let staged_files = extract_ccm_package(archive_path, &package, &staging)?;
        reporter.map(|reporter| reporter.status("backing-up", "Creating rollback snapshots before changing the campaign…"));
        ensure_shared_dependency_baselines(root, &dependency_roots_from_staged(&staged_files))?;
        let previous_install = snapshot_previous_install(root, &staging, &package.target_path)?;
        let previous_state = read_state_for_target(root, &package.target_path)?;
        let source_target = previous_state
            .as_ref()
            .map(ManagedState::target_path_or_first_clear)
            .unwrap_or_else(|| package.target_path.clone());
        let source_campaign_id = previous_state
            .as_ref()
            .map(|state| state.campaign_id.clone())
            .unwrap_or_default();
        let source_dependencies = previous_state
            .as_ref()
            .map(|state| dependency_roots_from_managed_files(&state.files))
            .unwrap_or_default();
        reporter.map(|reporter| reporter.status("switching-profile", "Saving and switching the selected StarCraft II profile…"));
        let profile_transaction = apply_profile_transition_with_hook(
            root,
            &source_target,
            &package.target_path,
            &source_campaign_id,
            &source_dependencies,
            &request.campaign_id,
            &dependency_roots_from_staged(&staged_files),
            &staging,
            request.profile_dir.as_deref(),
            |transaction, profile_roots| {
                write_pending_install_journal(
                    root,
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
        let profile_roots = read_pending_install_journal(root)?
            .map(|journal| journal.profile_roots)
            .unwrap_or_default();
        // A non-live/fixture install has no profile mutations, so its hook
        // never runs.  Persist the same intent before touching game files.
        write_pending_install_journal(
            root,
            &PendingInstallJournal {
                format: 1,
                profile_transaction: profile_transaction.clone(),
                profile_roots: profile_roots.clone(),
                previous_install: previous_install.clone(),
                new_state: None,
                completed: false,
            },
        )?;
        // All validation and extraction is complete. Only now can we inspect
        // the previous managed state and restore/remove its package-owned
        // files. A changed file is a hard stop; we never guess what is safe to
        // delete.
        reporter.map(|reporter| reporter.status("restoring-previous", "Restoring the previous campaign files safely…"));
        let previous = match restore_existing_campaign(root, &package.target_path) {
            Ok(previous) => previous,
            Err(error) => {
                if profile_transaction.rollback().is_ok() {
                    let _ = fs::remove_file(pending_install_path(root));
                }
                return Err(error);
            }
        };
        if !previous.conflicts.is_empty() {
            if profile_transaction.rollback().is_ok() {
                let _ = fs::remove_file(pending_install_path(root));
            }
            return Err("The active campaign has files changed outside CCM Reborn. Restore it manually before updating.".into());
        }
        let state = match backup_campaign_directory(root, request, &package.target_path, &staged_files) {
            Ok(state) => state,
            Err(error) => {
                if rollback_before_new_state(root, previous_install.as_ref(), &profile_transaction).is_ok() {
                    let _ = fs::remove_file(pending_install_path(root));
                }
                return Err(error);
            }
        };
        let manifest = installed_manifest_from_staged(root, request, &package, expected_hash, &staged_files);
        let manifest_path = installed_manifest_path(root, &package.target_path)?;
        // At this point the former managed campaign may already have been
        // restored away.  Persist both snapshots before clearing the target,
        // so recovery can restore the former mod rather than only vanilla.
        if let Err(error) = write_pending_install_journal(
            root,
            &PendingInstallJournal {
                format: 1,
                profile_transaction: profile_transaction.clone(),
                profile_roots: profile_roots.clone(),
                previous_install: previous_install.clone(),
                new_state: Some(state.clone()),
                completed: false,
            },
        ) {
            let _ = fs::remove_dir_all(manager_root(root).join(&state.backup_dir));
            if rollback_before_new_state(root, previous_install.as_ref(), &profile_transaction).is_ok() {
                let _ = fs::remove_file(pending_install_path(root));
            }
            return Err(error);
        }
        // Kept for compatibility with installations created before the
        // profile-aware journal. `pending-install.json` is authoritative here.
        if let Err(error) = write_json_atomic(&journal_path(root), &state) {
            let _ = fs::remove_dir_all(manager_root(root).join(&state.backup_dir));
            if rollback_before_new_state(root, previous_install.as_ref(), &profile_transaction).is_ok() {
                let _ = fs::remove_file(pending_install_path(root));
            }
            return Err(error);
        }
        reporter.map(|reporter| reporter.status("replacing-files", "Replacing campaign files…"));
        for directory in &state.cleared_directories {
            if let Err(error) = clear_campaign_target(&root, directory) {
                if rollback_install_failure(root, &state, &manifest_path, previous_install.as_ref(), &profile_transaction).is_ok() {
                    let _ = fs::remove_file(pending_install_path(root));
                }
                return Err(error);
            }
        }
        if let Err(error) = clear_dependency_roots(root, &dependency_roots_from_staged(&staged_files)) {
            if rollback_install_failure(root, &state, &manifest_path, previous_install.as_ref(), &profile_transaction).is_ok() {
                let _ = fs::remove_file(pending_install_path(root));
            }
            return Err(error);
        }
        let total_files = staged_files.len();
        for (index, staged) in staged_files.iter().enumerate() {
            let destination = safe_game_path(root, Path::new(&staged.destination))?;
            if let Err(error) = copy_file(&staged.path, &destination) {
                if rollback_install_failure(root, &state, &manifest_path, previous_install.as_ref(), &profile_transaction).is_ok() {
                    let _ = fs::remove_file(pending_install_path(root));
                }
                return Err(error);
            }
            reporter.map(|reporter| reporter.files("replacing-files", "Copying package files…", index + 1, total_files));
        }
        if let Err(error) = write_installed_manifest_atomic(&manifest_path, &manifest) {
            if rollback_install_failure(root, &state, &manifest_path, previous_install.as_ref(), &profile_transaction).is_ok() {
                let _ = fs::remove_file(pending_install_path(root));
            }
            return Err(error);
        }
        if let Err(error) = write_state_for_target(root, &state) {
            if rollback_install_failure(root, &state, &manifest_path, previous_install.as_ref(), &profile_transaction).is_ok() {
                let _ = fs::remove_file(pending_install_path(root));
            }
            return Err(error);
        }
        let completed = PendingInstallJournal {
            format: 1,
            profile_transaction: profile_transaction.clone(),
            profile_roots,
            previous_install: previous_install.clone(),
            new_state: Some(state.clone()),
            completed: true,
        };
        write_pending_install_journal(root, &completed)?;
        let _ = fs::remove_file(journal_path(root));
        let _ = fs::remove_file(pending_install_path(root));
        reporter.map(|reporter| reporter.status("finalizing", "Finalizing installation…"));
        Ok(InstallResult {
            campaign_id: state.campaign_id,
            title: state.title,
            version: state.version,
            manifest_path: manifest_path.display().to_string(),
            package_sha256: expected_hash.to_string(),
            files_installed: staged_files.len(),
        })
    })();
    // Keep staging data if rollback itself failed: the pending journal refers
    // to those backups and recovery on the next command needs them.
    if !pending_install_path(root).exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    if result.is_ok() {
        reporter.map(|reporter| reporter.status("completed", "Installation completed successfully."));
    }
    result
}
struct StagedFile {
    destination: String,
    source: String,
    path: PathBuf,
    sha256: String,
}
struct CcmPackage {
    content_prefix: String,
    target_path: String,
    metadata_text: String,
    // True for the "special" whole-game-root archive layout: top-level Maps/
    // and Mods/ folders with metadata.txt living inside the campaign slot.
    install_root: bool,
}
struct PackageFilePlan {
    source: String,
    destination: String,
    size: u64,
}
fn installed_manifest_path(root: &Path, target_path: &str) -> Result<PathBuf, String> {
    let target = safe_campaign_target(target_path)?;
    let slot = path_string(&target).replace('/', "_");
    Ok(manager_root(root).join("installed").join(format!("{slot}.json")))
}

fn installed_manifest_from_staged(
    _root: &Path,
    request: &InstallRequest,
    package: &CcmPackage,
    package_sha256: &str,
    staged_files: &[StagedFile],
) -> InstalledManifest {
    InstalledManifest {
        schema_version: INSTALLED_MANIFEST_SCHEMA_VERSION,
        campaign_id: request.campaign_id.clone(),
        title: request.title.trim().to_string(),
        author: request.author.trim().to_string(),
        version: request.version.trim().to_string(),
        package_sha256: package_sha256.to_string(),
        target_path: package.target_path.clone(),
        installed_at: unix_timestamp(),
        files: staged_files
            .iter()
            .map(|file| InstalledFile {
                destination: file.destination.clone(),
                source: file.source.clone(),
                size: fs::metadata(&file.path).map(|metadata| metadata.len()).unwrap_or(0),
                sha256: file.sha256.clone(),
                kind: if file.destination.starts_with("Mods/") {
                    "package dependency".into()
                } else {
                    "package campaign file".into()
                },
            })
            .collect(),
    }
}

fn inspect_ccm_package_files(
    archive_path: &Path,
    package: &CcmPackage,
) -> Result<Vec<PackageFilePlan>, String> {
    let file = File::open(archive_path).map_err(io_error)?;
    let mut archive = ZipArchive::new(file).map_err(zip_error)?;
    validate_archive_entry_count(archive.len())?;
    let mut planned = Vec::new();
    for index in 0..archive.len() {
        let source = archive.by_index(index).map_err(zip_error)?;
        if source.is_dir() {
            continue;
        }
        let source_name = source.name().to_string();
        let Some(relative) = package_member_relative(&package, &source_name) else {
            continue;
        };
        if source.size() > MAX_FILE_BYTES {
            return Err(format!("Package file {source_name} is too large."));
        }
        let relative = safe_relative_path(relative)?;
        let destination = package_destination(&package, &relative)?;
        planned.push(PackageFilePlan {
            source: source_name,
            destination: path_string(&destination),
            size: source.size(),
        });
    }
    if planned.is_empty() {
        return Err("CCM package contains no files alongside metadata.txt.".into());
    }
    Ok(planned)
}

fn refuse_nested_wol_package(target: &str, files: &[PackageFilePlan]) -> Result<(), String> {
    if target != "Maps/Campaign" {
        return Ok(());
    }
    let wol_root = Path::new("Maps/Campaign");
    for file in files {
        let destination = Path::new(&file.destination);
        let Ok(relative) = destination.strip_prefix(wol_root) else {
            continue;
        };
        let mut components = relative.components();
        let first = components.next().and_then(|component| match component {
            Component::Normal(name) => Some(name),
            _ => None,
        });
        if components.next().is_some() && !first.is_some_and(is_wol_owned_asset_directory) {
            return Err(format!(
                "WoL package file {} targets an ambiguous nested Maps/Campaign directory. Nested assets must live inside a .SC2Map or .SC2Mod directory so CCM cannot overwrite another campaign branch.",
                file.source
            ));
        }
    }
    Ok(())
}

fn dependency_roots_from_planned_files(files: &[PackageFilePlan]) -> Vec<PathBuf> {
    files
        .iter()
        .filter_map(|file| dependency_root_from_destination(Path::new(&file.destination)))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect()
}

fn read_ccm_package(archive_path: &Path) -> Result<CcmPackage, String> {
    let file = File::open(archive_path).map_err(io_error)?;
    let mut archive = ZipArchive::new(file).map_err(zip_error)?;
    validate_archive_entry_count(archive.len())?;
    let mut metadata_indexes = Vec::new();
    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(zip_error)?;
        let name = file.name();
        if name.contains('\\') {
            return Err("CCM packages must use forward slashes in ZIP paths.".into());
        }
        if is_metadata_file(name) {
            metadata_indexes.push(index);
        }
    }
    if metadata_indexes.len() != 1 {
        return Err("A CCM package must contain exactly one metadata.txt file.".into());
    }
    let mut metadata = archive.by_index(metadata_indexes[0]).map_err(zip_error)?;
    if metadata.size() > MAX_METADATA_BYTES {
        return Err("metadata.txt is too large.".into());
    }
    let metadata_name = metadata.name().to_string();
    let mut metadata_bytes = Vec::new();
    metadata.read_to_end(&mut metadata_bytes).map_err(io_error)?;
    let metadata_text = decode_ccm_metadata(&metadata_bytes)?;
    let target_path = campaign_target_from_metadata(&metadata_text)?;
    let (content_prefix, install_root) = resolve_package_layout(&metadata_name, &target_path)?;
    Ok(CcmPackage {
        content_prefix,
        target_path,
        metadata_text,
        install_root,
    })
}

fn extract_ccm_package(
    archive_path: &Path,
    package: &CcmPackage,
    staging: &Path,
) -> Result<Vec<StagedFile>, String> {
    let file = File::open(archive_path).map_err(io_error)?;
    let mut archive = ZipArchive::new(file).map_err(zip_error)?;
    validate_archive_entry_count(archive.len())?;
    let mut staged_files = Vec::new();
    let mut total_size = 0_u64;

    for index in 0..archive.len() {
        let mut source = archive.by_index(index).map_err(zip_error)?;
        if source.is_dir() {
            continue;
        }
        let source_name = source.name().to_string();
        let Some(relative) = package_member_relative(&package, &source_name) else {
            continue;
        };
        if source.size() > MAX_FILE_BYTES {
            return Err(format!("Package file {source_name} is too large."));
        }
        total_size = total_size.saturating_add(source.size());
        if total_size > MAX_ARCHIVE_BYTES {
            return Err("Package expands beyond the allowed size.".into());
        }

        let relative = safe_relative_path(relative)?;
        let destination = package_destination(&package, &relative)?;
        let destination_string = path_string(&destination);
        let staged_path = staging.join(&destination);
        if let Some(parent) = staged_path.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        let expected_size = source.size();
        let (copied, sha256) = write_reader_hashed(&mut source, &staged_path)?;
        if copied != expected_size {
            return Err(format!("Could not fully extract {source_name}."));
        }
        staged_files.push(StagedFile {
            destination: destination_string,
            source: source_name,
            sha256,
            path: staged_path,
        });
    }
    if staged_files.is_empty() {
        return Err("CCM package contains no files alongside metadata.txt.".into());
    }
    Ok(staged_files)
}

fn backup_campaign_directory(
    root: &Path,
    request: &InstallRequest,
    target_path: &str,
    staged_files: &[StagedFile],
) -> Result<ManagedState, String> {
    let backup_dir = format!("backups/{}-{}", request.campaign_id, Uuid::new_v4());
    let backup_root = manager_root(root).join(&backup_dir);
    let mut files = Vec::new();
    let mut positions = HashMap::new();

    let campaign_roots = campaign_roots_for_staged_files(target_path, staged_files)?;
    for campaign_root in &campaign_roots {
        for original in collect_campaign_target_files(root, &path_string(campaign_root))? {
            record_original_file(
                &original,
                root,
                &backup_root,
                &backup_dir,
                &mut files,
                &mut positions,
            )?;
        }
    }
    for dependency_root in dependency_roots_from_staged(staged_files) {
        for original in collect_regular_files_or_file(&safe_game_path(root, &dependency_root)?)? {
            record_original_file(
                &original,
                root,
                &backup_root,
                &backup_dir,
                &mut files,
                &mut positions,
            )?;
        }
}
    for staged in staged_files {
        if let Some(position) = positions.get(&staged.destination) {
            files[*position].installed_sha256 = Some(staged.sha256.clone());
        } else {
            positions.insert(staged.destination.clone(), files.len());
            files.push(ManagedFile {
                destination: staged.destination.clone(),
                original_existed: false,
                backup_path: None,
                installed_sha256: Some(staged.sha256.clone()),
            });
        }
    }

    Ok(ManagedState {
        format: 1,
        campaign_id: request.campaign_id.clone(),
        title: request.title.trim().to_string(),
        author: request.author.trim().to_string(),
        version: request.version.trim().to_string(),
        target_path: target_path.to_string(),
        installed_at: unix_timestamp(),
        backup_dir,
        cleared_directories: campaign_roots.into_iter().map(|root| path_string(&root)).collect(),
        files,
    })
}

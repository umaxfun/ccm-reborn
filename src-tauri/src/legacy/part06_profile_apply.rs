#[allow(dead_code)]
fn apply_profile_transition(
    root: &Path,
    source_target: &str,
    target: &str,
    source_campaign_id: &str,
    source_dependencies: &[PathBuf],
    target_campaign_id: &str,
    target_dependencies: &[PathBuf],
    staging: &Path,
    profile_dir: Option<&str>,
) -> Result<ProfileTransaction, String> {
    apply_profile_transition_with_hook(
        root,
        source_target,
        target,
        source_campaign_id,
        source_dependencies,
        target_campaign_id,
        target_dependencies,
        staging,
        profile_dir,
        |_, _| Ok(()),
    )
}

/// Apply a profile switch, persisting the captured rollback transaction before
/// the first profile file is changed.  The install flow uses the hook to make
/// interrupted profile transitions recoverable; other callers use the small
/// wrapper above.
fn apply_profile_transition_with_hook<F>(
    root: &Path,
    source_target: &str,
    target: &str,
    source_campaign_id: &str,
    source_dependencies: &[PathBuf],
    target_campaign_id: &str,
    target_dependencies: &[PathBuf],
    staging: &Path,
    profile_dir: Option<&str>,
    before_apply: F,
) -> Result<ProfileTransaction, String>
where
    F: FnOnce(&ProfileTransaction, &[PathBuf]) -> Result<(), String>,
{
    if campaign_profile_spec(target).is_none() || campaign_profile_spec(source_target).is_none() {
        return Ok(ProfileTransaction { entries: Vec::new() });
    }
    if !looks_like_starcraft(root) {
        return Ok(ProfileTransaction { entries: Vec::new() });
    }
    let Some(discovered) = select_starcraft_profile(profile_dir)? else {
        return Ok(ProfileTransaction { entries: Vec::new() });
    };
    reject_ambiguous_legacy_profile_store(source_campaign_id)?;
    reject_ambiguous_legacy_profile_store(target_campaign_id)?;
    apply_profile_transition_at_with_hook(
        root,
        &discovered.path,
        &profile_store_base_for(&discovered.path)?,
        source_target,
        target,
        source_campaign_id,
        source_dependencies,
        target_campaign_id,
        target_dependencies,
        staging,
        before_apply,
    )
}

#[allow(dead_code)]
fn apply_profile_transition_at(
    _root: &Path,
    profile_path: &Path,
    profile_base: &Path,
    source_target: &str,
    target: &str,
    source_campaign_id: &str,
    source_dependencies: &[PathBuf],
    target_campaign_id: &str,
    target_dependencies: &[PathBuf],
    staging: &Path,
) -> Result<ProfileTransaction, String> {
    apply_profile_transition_at_with_hook(
        _root,
        profile_path,
        profile_base,
        source_target,
        target,
        source_campaign_id,
        source_dependencies,
        target_campaign_id,
        target_dependencies,
        staging,
        |_, _| Ok(()),
    )
}

fn apply_profile_transition_at_with_hook<F>(
    _root: &Path,
    profile_path: &Path,
    profile_base: &Path,
    source_target: &str,
    target: &str,
    source_campaign_id: &str,
    source_dependencies: &[PathBuf],
    target_campaign_id: &str,
    target_dependencies: &[PathBuf],
    staging: &Path,
    before_apply: F,
) -> Result<ProfileTransaction, String>
where
    F: FnOnce(&ProfileTransaction, &[PathBuf]) -> Result<(), String>,
{
    validate_profile_switch_roots(profile_path, profile_base)?;
    let Some(target_spec) = campaign_profile_spec(target) else {
        return Ok(ProfileTransaction { entries: Vec::new() });
    };
    let Some(source_spec) = campaign_profile_spec(source_target) else {
        return Ok(ProfileTransaction { entries: Vec::new() });
    };
    let profile = profile_path;
    let source_key = if source_campaign_id.is_empty() {
        format!("vanilla-{}", campaign_slot_name(source_target)?)
    } else {
        source_campaign_id.to_string()
    };
    let source_store = profile_base.join(&source_key);
    let target_store = profile_base.join(target_campaign_id);
    validate_profile_switch_roots(profile_path, &source_store)?;
    validate_profile_switch_roots(profile_path, &target_store)?;
    let source_dependency_names = source_dependencies
        .iter()
        .filter_map(|path| path.file_name().and_then(|name| name.to_str()))
        .map(|name| name.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let target_dependency_names = target_dependencies
        .iter()
        .filter_map(|path| path.file_name().and_then(|name| name.to_str()))
        .map(|name| name.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let mut target_profile_dependency_names = target_dependency_names.clone();
    if source_campaign_id == target_campaign_id {
        for dependency in &source_dependency_names {
            if !target_profile_dependency_names.contains(dependency) {
                target_profile_dependency_names.push(dependency.clone());
            }
        }
    }
    let source_allow_unmanaged = source_campaign_id.is_empty();
    let target_allow_unmanaged = target_campaign_id.starts_with("vanilla-");
    let source_saves = campaign_owned_save_paths(profile, &source_spec, &source_dependency_names, source_allow_unmanaged)?;
    let target_saves = campaign_owned_save_paths(profile, &target_spec, &target_profile_dependency_names, target_allow_unmanaged)?;
    let source_store_saves = campaign_owned_save_paths(&source_store, &source_spec, &source_dependency_names, source_allow_unmanaged)?;
    let target_store_saves = campaign_owned_save_paths(&target_store, &target_spec, &target_profile_dependency_names, target_allow_unmanaged)?;
    let progress_path = profile.join("CampaignProgress.xml");
    let source_banks = campaign_slot_bank_paths(profile, &source_spec);
    let target_banks = campaign_slot_bank_paths(profile, &target_spec);
    let source_store_progress = source_store.join("CampaignProgress.xml");
    let source_store_resume_manifest = source_store.join("ccm-resume.json");
    let source_store_banks = campaign_slot_bank_paths(&source_store, &source_spec);
    let target_store_banks = campaign_slot_bank_paths(&target_store, &target_spec);
    let mut touched = vec![progress_path.clone(), source_store_progress.clone(), source_store_resume_manifest.clone()];
    touched.extend(source_banks.iter().cloned());
    touched.extend(target_banks.iter().cloned());
    touched.extend(source_store_banks.iter().cloned());
    touched.extend(target_store_banks.iter().cloned());
    touched.extend(source_saves.iter().cloned());
    touched.extend(target_saves.iter().cloned());
    touched.extend(source_store_saves.iter().cloned());
    touched.extend(source_saves.iter().filter_map(|path| profile_store_file(&source_store, profile, path).ok()));
    let transaction = ProfileTransaction::capture(touched, staging)?;
    // This must stay immediately before `operation`: if the process dies
    // after this line, `pending-install.json` contains the original bytes of
    // every profile file about to be touched.
    before_apply(
        &transaction,
        &[profile_path.to_path_buf(), profile_base.to_path_buf()],
    )?;
    let operation = (|| {
        for stale in &source_store_saves {
            remove_profile_file(stale)?;
        }
        if progress_path.is_file() {
            copy_file(&progress_path, &source_store_progress)?;
        }
        for (source_bank, source_store_bank) in source_banks.iter().zip(source_store_banks.iter()) {
            if source_bank.is_file() {
                copy_file(source_bank, source_store_bank)?;
            }
        }
        for save in &source_saves {
            copy_file(save, &profile_store_file(&source_store, profile, save)?)?;
        }
        if !source_campaign_id.is_empty() {
            write_json_atomic(
                &source_store_resume_manifest,
                &CampaignProfileResumeManifest {
                    format: 1,
                    campaign_id: source_campaign_id.to_string(),
                    target_path: source_target.to_string(),
                    dependency_names: source_dependency_names.clone(),
                    captured_at: unix_timestamp(),
                },
            )?;
        }
        for save in &source_saves {
            remove_profile_file(save)?;
        }
        for source_bank in &source_banks {
            remove_profile_file(source_bank)?;
        }
        for save in &target_saves {
            remove_profile_file(save)?;
        }
        for target_bank in &target_banks {
            remove_profile_file(target_bank)?;
        }

        for (target_store_bank, target_bank) in target_store_banks.iter().zip(target_banks.iter()) {
            if target_store_bank.is_file() {
                copy_file(target_store_bank, target_bank)?;
            }
        }
        for save in &target_store_saves {
            let live = profile.join(save.strip_prefix(&target_store).map_err(|_| "Target profile save escaped its store.")?);
            copy_file(save, &live)?;
        }

        if progress_path.is_file() {
            let current = fs::read_to_string(&progress_path).map_err(io_error)?;
            let updated = if target_store_progress_path(&target_store).is_file() {
                let stored = fs::read_to_string(target_store_progress_path(&target_store)).map_err(io_error)?;
                merge_progress_node(&current, &stored, &target_spec).0
            } else {
                reset_progress_node(&current, &target_spec).0
            };
            if updated != current {
                write_text_atomic(&progress_path, &updated)?;
            }
        }
        Ok::<(), String>(())
    })();
    if let Err(error) = operation {
        let rollback_error = transaction.rollback().err();
        return Err(match rollback_error {
            Some(rollback_error) => format!("{error}; profile rollback also failed: {rollback_error}"),
            None => error,
        });
    }
    Ok(transaction)
}

fn target_store_progress_path(store: &Path) -> PathBuf {
    store.join("CampaignProgress.xml")
}

fn inspect_bank_shape(path: &Path) -> Result<(usize, usize), String> {
    let text = fs::read_to_string(path).map_err(io_error)?;
    Ok((
        text.lines().filter(|line| line.contains("<Section name=")).count(),
        text.lines().filter(|line| line.contains("<Key name=")).count(),
    ))
}

fn campaign_progress_key_changes(line: &str) -> Vec<ProgressKeyChange> {
    ["tutorialfinished", "campaignfinished"]
        .iter()
        .filter_map(|key| {
            let marker = format!("{key}=");
            let start = line.find(&marker)? + marker.len();
            let remainder = &line[start..];
            let quote = remainder.chars().next()?;
            let end = remainder[1..].find(quote)? + 1;
            let current_value = remainder[1..end].to_string();
            Some(ProgressKeyChange {
                key: (*key).into(),
                current_value,
                planned_value: "0".into(),
                action: "reset target campaign node for clean mod profile".into(),
            })
        })
        .collect()
}

fn campaign_progress_key_changes_between(current: &str, target: &str) -> Vec<ProgressKeyChange> {
    ["tutorialfinished", "campaignfinished"]
        .iter()
        .filter_map(|key| {
            let read_value = |line: &str| {
                let marker = format!("{key}=");
                let start = line.find(&marker)? + marker.len();
                let remainder = &line[start..];
                let quote = remainder.chars().next()?;
                let end = remainder[1..].find(quote)? + 1;
                Some(remainder[1..end].to_string())
            };
            Some(ProgressKeyChange {
                key: (*key).into(),
                current_value: read_value(current)?,
                planned_value: read_value(target)?,
                action: "restore target campaign node from its saved profile".into(),
            })
        })
        .collect()
}

fn push_profile_file(
    profile: &Path,
    path: &Path,
    kind: &str,
    action: &str,
    detail: Option<String>,
    destination_root: &Path,
    files: &mut Vec<ProgressFilePlan>,
) -> Result<(), String> {
    push_profile_file_from_root(profile, path, kind, action, detail, destination_root, files)
}

fn push_profile_file_from_root(
    source_root: &Path,
    path: &Path,
    kind: &str,
    action: &str,
    detail: Option<String>,
    destination_root: &Path,
    files: &mut Vec<ProgressFilePlan>,
) -> Result<(), String> {
    let metadata = fs::metadata(path).map_err(io_error)?;
    let relative_path = path_string(path.strip_prefix(source_root).map_err(|_| "Profile path escaped its root.")?);
    files.push(ProgressFilePlan {
        source: path.display().to_string(),
        destination: destination_root.join(&relative_path).display().to_string(),
        relative_path,
        kind: kind.into(),
        action: action.into(),
        size: metadata.len(),
        sha256: sha256_file(path)?,
        detail,
    });
    Ok(())
}

fn inspect_save_details(path: &Path) -> Result<SaveDetails, String> {
    let mut archive = std::panic::catch_unwind(|| mpq::Archive::open(path))
        .map_err(|_| "MPQ reader rejected the archive.".to_string())?
        .map_err(|error| format!("MPQ reader error: {error}"))?;
    let file = archive
        .open_file("save.details")
        .map_err(|error| format!("save.details is unavailable: {error}"))?;
    let size = file.size() as usize;
    if size > 16 * 1024 * 1024 {
        return Err("save.details is larger than the safe inspection limit.".into());
    }
    let mut bytes = vec![0; size];
    let read = file
        .read(&mut archive, &mut bytes)
        .map_err(|error| format!("save.details could not be decoded: {error}"))?;
    bytes.truncate(read);
    Ok(SaveDetails {
        maps: extract_archive_paths(&bytes, "Campaign/", ".SC2Map"),
        mods: extract_archive_paths(&bytes, "Mods/", ".SC2Mod"),
        campaigns: extract_archive_paths(&bytes, "Campaigns/", ".SC2Campaign"),
    })
}

fn extract_archive_paths(bytes: &[u8], marker: &str, suffix: &str) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut paths = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = text[offset..].find(marker) {
        let start = offset + relative_start;
        let Some(relative_end) = text[start..].find(suffix) else {
            break;
        };
        let end = start + relative_end + suffix.len();
        let candidate = &text[start..end];
        if candidate.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '/' | '_' | '-' | '.' | ' ')
        }) {
            let candidate = candidate.to_string();
            if !paths.contains(&candidate) {
                paths.push(candidate);
            }
        }
        offset = end;
    }
    paths
}

fn save_details_match_campaign(details: &SaveDetails, spec: &CampaignProfileSpec) -> bool {
    let marker = spec.save_marker.to_ascii_lowercase();
    details
        .maps
        .iter()
        .chain(details.campaigns.iter())
        .map(|value| value.to_ascii_lowercase())
        .any(|value| value.contains(&format!("campaign/{marker}")) || value.contains(&format!("campaigns/{marker}")))
}

fn save_details_match_dependencies(details: &SaveDetails, dependency_names: &[String]) -> bool {
    details.mods.iter().any(|mod_path| {
        let normalized = mod_path.to_ascii_lowercase();
        dependency_names
            .iter()
            .any(|dependency| normalized.ends_with(dependency))
    })
}

fn save_details_summary(details: &SaveDetails) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(map) = details.maps.first() {
        parts.push(format!("map: {map}"));
    }
    if !details.mods.is_empty() {
        parts.push(format!("mods: {}", details.mods.join(", ")));
    }
    if let Some(campaign) = details.campaigns.first() {
        parts.push(format!("campaign: {campaign}"));
    }
    (!parts.is_empty()).then_some(parts.join(" · "))
}

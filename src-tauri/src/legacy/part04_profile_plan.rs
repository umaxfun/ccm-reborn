fn plan_profile_transition(
    root: &Path,
    target: &str,
    source_campaign_id: &str,
    target_campaign_id: &str,
    current_title: Option<&str>,
    current_dependencies: &[PathBuf],
    profile_dir: Option<&str>,
) -> Result<ProfilePlan, String> {
    let Some(spec) = campaign_profile_spec(target) else {
        return Ok(ProfilePlan {
            profile_path: None,
            profile_store_path: None,
            files: Vec::new(),
            progress_keys: Vec::new(),
            bank_plans: Vec::new(),
            warnings: vec!["Campaign profile mapping is unknown; save and progress files will be left untouched.".into()],
        });
    };

    // A fixture or a user-selected Maps folder is not enough evidence that this
    // is the real game install. Avoid accidentally inspecting the user's home
    // profile while running tests or planning against an incomplete folder.
    if !looks_like_starcraft(root) {
        return Ok(ProfilePlan {
            profile_path: None,
            profile_store_path: None,
            files: Vec::new(),
            progress_keys: Vec::new(),
            bank_plans: Vec::new(),
            warnings: vec!["StarCraft II executable layout was not detected; account saves and progress were not scanned.".into()],
        });
    }

    let profile = match select_starcraft_profile(profile_dir)? {
        Some(profile) => profile,
        None => {
            return Ok(ProfilePlan {
                profile_path: None,
                profile_store_path: None,
                files: Vec::new(),
                progress_keys: Vec::new(),
                bank_plans: Vec::new(),
                warnings: vec!["No StarCraft II account profile was found; campaign saves and progress will be left untouched.".into()],
            });
        }
    };
    let mut warnings = Vec::new();

    let profile_label = display_profile_path(&profile.path);
    let profile_base = profile_store_base_for(&profile.path)?;
    let source_key = if source_campaign_id.is_empty() {
        format!("vanilla-{}", campaign_slot_name(target)?)
    } else {
        source_campaign_id.to_string()
    };
    let source_profile_store = profile_store_root(&profile_base, &source_key);
    let target_profile_store = profile_store_root(&profile_base, target_campaign_id);
    let profile_store_label = if source_profile_store == target_profile_store {
        display_profile_path(&target_profile_store)
    } else {
        format!("{} → {}", display_profile_path(&source_profile_store), display_profile_path(&target_profile_store))
    };
    if profile.other_profiles > 0 {
        warnings.push(format!(
            "{} StarCraft II profiles found; selected {} and will leave the others untouched.",
            profile.other_profiles + 1,
            profile_label
        ));
    }

    let mut files = Vec::new();
    let mut progress_keys = Vec::new();
    let mut bank_plans = Vec::new();
    let dependency_names = current_dependencies
        .iter()
        .filter_map(|path| path.file_name().and_then(|name| name.to_str()))
        .map(|name| name.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if !dependency_names.is_empty() {
        if let Some(title) = current_title {
            warnings.push(format!(
                "Save ownership is matched to the current managed campaign {title:?}; its profile is the destination for these snapshots. Saves from other campaigns remain untouched."
            ));
        }
    }
    for bank_path in campaign_slot_bank_paths(&profile.path, &spec).into_iter().filter(|path| path.is_file()) {
        let relative_path = path_string(bank_path.strip_prefix(&profile.path).map_err(|_| "Bank path escaped the profile.")?);
        let (sections, keys) = inspect_bank_shape(&bank_path)?;
        bank_plans.push(BankPlan {
            relative_path: relative_path.clone(),
            source: bank_path.display().to_string(),
            destination: source_profile_store.join(&relative_path).display().to_string(),
            sections,
            keys,
            keys_changed_in_place: 0,
            note: "The bank is snapshotted and swapped as a whole; no existing bank key is mutated in place.".into(),
        });
        push_profile_file(
            &profile.path,
            &bank_path,
            "bank",
            "snapshot-current-and-reset-for-mod",
            Some(format!("{} campaign bank", spec.save_marker)),
            &source_profile_store,
            &mut files,
        )?;
    }

    let progress_path = profile.path.join("CampaignProgress.xml");
    let mut current_progress_line = None;
    if progress_path.is_file() {
        let xml = fs::read_to_string(&progress_path).map_err(io_error)?;
        let matching_line = spec.progress_ids.iter().find_map(|id| {
            xml.lines()
                .find(|line| line.contains(&format!("id=\"{id}\"")) || line.contains(&format!("id='{id}'")))
                .map(str::trim)
        });
        if let Some(line) = matching_line {
            current_progress_line = Some(line.to_string());
            progress_keys = campaign_progress_key_changes(line);
            push_profile_file(
                &profile.path,
                &progress_path,
                "campaign-progress",
                "snapshot-and-reset-target-node",
                Some(format!("current node: {line}")),
                &source_profile_store,
                &mut files,
            )?;
        } else {
            warnings.push(format!(
                "CampaignProgress.xml has no {} node; it will not be rewritten automatically.",
                spec.save_marker
            ));
        }
    } else {
        warnings.push("CampaignProgress.xml was not found; only discovered bank/save files can be profiled.".into());
    }

    let saves_root = profile.path.join("Saves");
    let save_paths = collect_regular_files(&saves_root)?;
    for save_path in save_paths {
        if save_path.extension().and_then(|extension| extension.to_str()).map(|extension| extension.eq_ignore_ascii_case("SC2Save")) != Some(true) {
            continue;
        }
        let relative = save_path
            .strip_prefix(&profile.path)
            .map_err(|_| "Save path escaped the StarCraft II profile.")?;
        let relative_text = path_string(relative);
        let filename_matches = relative
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| save_filename_matches(name, &spec))
            .unwrap_or(false);
        let is_campaign_folder = relative_text.to_ascii_lowercase().starts_with("saves/campaign/")
            || relative_text.to_ascii_lowercase().starts_with("saves/unsaved/campaign/");
        let (belongs, detail) = if filename_matches {
            (true, Some("slot-named campaign state; moved even when save.details is unavailable".into()))
        } else if is_campaign_folder {
            match inspect_save_details(&save_path) {
                Ok(details) => {
                    let belongs = save_details_match_campaign(&details, &spec)
                        && save_details_match_dependencies(&details, &dependency_names);
                    let detail = save_details_summary(&details);
                    (belongs, detail)
                }
                Err(error) => {
                    warnings.push(format!("Could not inspect {relative_text}: {error}; it will be left untouched."));
                    (false, None)
                }
            }
        } else {
            (false, None)
        };
        if belongs {
            push_profile_file(
                &profile.path,
                &save_path,
                "campaign-save",
                "snapshot-and-move-to-mod-profile",
                detail,
                &source_profile_store,
                &mut files,
            )?;
        }
    }

    // Target profiles are only populated by CCM itself. Unlike live saves,
    // they no longer need heuristic ownership detection: every item here is
    // an exact previous snapshot of the selected target campaign.
    for target_bank in campaign_slot_bank_paths(&target_profile_store, &spec).into_iter().filter(|path| path.is_file()) {
        let relative_path = path_string(target_bank.strip_prefix(&target_profile_store).map_err(|_| "Target profile bank escaped its store.")?);
        let (sections, keys) = inspect_bank_shape(&target_bank)?;
        bank_plans.push(BankPlan {
            relative_path: relative_path.clone(),
            source: target_bank.display().to_string(),
            destination: profile.path.join(&relative_path).display().to_string(),
            sections,
            keys,
            keys_changed_in_place: 0,
            note: "The saved target-campaign bank is restored as a whole; no bank key is edited in place.".into(),
        });
        push_profile_file_from_root(
            &target_profile_store,
            &target_bank,
            "bank",
            "restore-target-mod-profile-bank",
            Some(format!("restore saved {} campaign bank", spec.save_marker)),
            &profile.path,
            &mut files,
        )?;
    }
    let target_progress = target_store_progress_path(&target_profile_store);
    if target_progress.is_file() {
        let target_xml = fs::read_to_string(&target_progress).map_err(io_error)?;
        if let Some(target_line) = target_xml
            .lines()
            .find(|line| progress_node_matches(line, &spec))
            .map(str::trim)
        {
            if let Some(current_line) = current_progress_line.as_deref() {
                progress_keys = campaign_progress_key_changes_between(current_line, target_line);
            }
            push_profile_file_from_root(
                &target_profile_store,
                &target_progress,
                "campaign-progress",
                "restore-target-campaign-progress-node",
                Some(format!("restore target node: {target_line}")),
                &profile.path,
                &mut files,
            )?;
        }
    }
    for stored_save in collect_regular_files(&target_profile_store.join("Saves"))? {
        if stored_save.extension().and_then(|extension| extension.to_str()).map(|extension| extension.eq_ignore_ascii_case("SC2Save")) != Some(true) {
            continue;
        }
        push_profile_file_from_root(
            &target_profile_store,
            &stored_save,
            "campaign-save",
            "restore-target-mod-profile-save",
            Some("restore exact save previously stored for this campaign".into()),
            &profile.path,
            &mut files,
        )?;
    }

    if dependency_names.is_empty() {
        warnings.push(
            "No managed dependency was found for the current campaign; individual campaign saves stay untouched until their mod identity is known.".into(),
        );
    }
    let restores_target_profile = files.iter().any(|file| file.action.starts_with("restore"));
    if files.is_empty() {
        warnings.push(format!(
            "No {} bank, save, or progress files were found in the selected profile.",
            spec.save_marker
        ));
    } else if restores_target_profile {
        warnings.push(format!(
            "Saved {} profile data will be restored after the current campaign profile is snapshotted.",
            spec.save_marker
        ));
    } else {
        warnings.push(format!(
            "The new {} mod profile will start clean: current bank/save files are snapshotted, and only the target campaign progress node is reset.",
            spec.save_marker
        ));
    }
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(ProfilePlan {
        profile_path: Some(profile_label),
        profile_store_path: Some(profile_store_label),
        files,
        progress_keys,
        bank_plans,
        warnings,
    })
}


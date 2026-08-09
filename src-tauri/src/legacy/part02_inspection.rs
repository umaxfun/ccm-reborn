#[tauri::command]
async fn inspect_game_directory(
    game_dir: String,
    known_campaigns: Option<Vec<KnownCampaign>>,
) -> Result<GameInspection, String> {
    tauri::async_runtime::spawn_blocking(move || inspect_game_directory_blocking(game_dir, known_campaigns))
        .await
        .map_err(|error| format!("Campaign inspection worker failed: {error}"))?
}

fn inspect_game_directory_blocking(
    game_dir: String,
    known_campaigns: Option<Vec<KnownCampaign>>,
) -> Result<GameInspection, String> {
    let root = PathBuf::from(game_dir.trim());
    if !root.exists() {
        return Ok(GameInspection {
            exists: false,
            path: root.display().to_string(),
            looks_like_starcraft: false,
            active_campaign: None,
            managed_campaigns: Vec::new(),
            active_campaigns: Vec::new(),
            can_launch: false,
            recovery_performed: false,
        });
    }

    // Inspection is read-only. In particular, opening/refreshing the UI must
    // never roll back files underneath a running StarCraft II process.
    // Recovery is performed only by an explicit mutating operation after its
    // game-closed precondition has been checked.
    let recovery_performed = false;
    let mut managed_campaigns = read_managed_states(&root)?
        .into_iter()
        .map(|state| {
            let target = state.target_path_or_first_clear();
            ActiveCampaign {
                id: state.campaign_id,
                title: state.title,
                slot: campaign_slot_for_target(&target).unwrap_or_default().to_string(),
                target_path: target,
                files: state.files.len(),
            }
        })
        .collect::<Vec<_>>();
    managed_campaigns.sort_by(|left, right| left.slot.cmp(&right.slot));
    let active_campaign = managed_campaigns.first().cloned();
    let mut active_campaigns = inspect_current_campaigns(&root)?;
    identify_catalog_campaigns(&root, &mut active_campaigns, &known_campaigns.unwrap_or_default());
    Ok(GameInspection {
        exists: true,
        path: root.display().to_string(),
        looks_like_starcraft: looks_like_starcraft(&root),
        active_campaign,
        managed_campaigns,
        active_campaigns,
        can_launch: can_launch_starcraft(&root),
        recovery_performed,
    })
}

#[tauri::command]
fn launch_current_campaign(game_dir: String) -> Result<LaunchResult, String> {
    let root = require_desktop_game_root(&game_dir)?;
    launch_starcraft(&root)?;
    Ok(LaunchResult {
        message: "StarCraft II launched. Choose Continue to resume the current campaign.".into(),
    })
}

#[tauri::command]
async fn inspect_saved_campaign_resumes(
    game_dir: Option<String>,
    profile_dir: Option<String>,
) -> Result<Vec<SavedCampaignResume>, String> {
    tauri::async_runtime::spawn_blocking(move || inspect_saved_campaign_resumes_blocking(game_dir, profile_dir))
        .await
        .map_err(|error| format!("Saved campaign inventory worker failed: {error}"))?
}

fn inspect_saved_campaign_resumes_blocking(
    game_dir: Option<String>,
    profile_dir: Option<String>,
) -> Result<Vec<SavedCampaignResume>, String> {
    let root = match select_starcraft_profile(profile_dir.as_deref())? {
        Some(profile) => profile_store_base_for(&profile.path)?,
        None => return Ok(Vec::new()),
    };
    let mut resumes = inspect_saved_campaign_resumes_at(&root)?;
    if let Some(game_dir) = game_dir.as_deref().map(str::trim).filter(|path| !path.is_empty()) {
        for live_resume in inspect_live_active_campaign_resumes(&PathBuf::from(game_dir), profile_dir.as_deref())? {
            if let Some(stored) = resumes.iter_mut().find(|stored| stored.campaign_id == live_resume.campaign_id) {
                let should_replace = live_resume
                    .latest_save
                    .as_ref()
                    .map(|save| save.modified_at)
                    .unwrap_or_default()
                    > stored.latest_save.as_ref().map(|save| save.modified_at).unwrap_or_default();
                if should_replace || stored.latest_save.is_none() {
                    *stored = live_resume;
                }
            } else {
                resumes.push(live_resume);
            }
        }
    }
    sort_saved_campaign_resumes(&mut resumes);
    Ok(resumes)
}

fn inspect_saved_campaign_resumes_at(root: &Path) -> Result<Vec<SavedCampaignResume>, String> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut resumes = Vec::new();
    for entry in fs::read_dir(root).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let file_type = entry.file_type().map_err(io_error)?;
        if file_type.is_symlink() || !file_type.is_dir() {
            continue;
        }
        let campaign_id = entry.file_name().to_string_lossy().to_string();
        if validate_campaign_id(&campaign_id).is_err() {
            continue;
        }
        let store = entry.path();
        let mut saves = collect_regular_files(&store.join("Saves"))?
            .into_iter()
            .filter(|path| path.extension().and_then(|extension| extension.to_str()).is_some_and(|extension| extension.eq_ignore_ascii_case("SC2Save")))
            // Root-level *CampaignSave / *PublishArchive files are SC2's
            // slot state, not a checkpoint a player should be asked to load.
            .filter(|path| is_user_loadable_campaign_save(&store, path))
            .collect::<Vec<_>>();
        saves.sort_by(|left, right| {
            save_modified_at(right)
                .cmp(&save_modified_at(left))
                .then_with(|| left.cmp(right))
        });
        let manifest_path = store.join("ccm-resume.json");
        let manifest = read_campaign_profile_resume_manifest(&manifest_path, &campaign_id).ok();
        let mut unverified_save_count = 0usize;
        let latest_save = if let Some(manifest) = manifest.filter(|manifest| !manifest.dependency_names.is_empty()) {
            saves.iter().find_map(|path| {
                let details = inspect_save_details(path).ok()?;
                if !save_details_match_dependencies(&details, &manifest.dependency_names) {
                    return None;
                }
                let relative_path = path_string(path.strip_prefix(&store).ok()?);
                Some(SavedCampaignSave {
                    relative_path,
                    modified_at: save_modified_at(path),
                    map: details.maps.first().cloned(),
                    details_available: true,
                })
            })
        } else {
            unverified_save_count = saves.len();
            None
        };
        if latest_save.is_none() {
            unverified_save_count = saves.len();
        }
        if !saves.is_empty() || manifest_path.is_file() {
            resumes.push(SavedCampaignResume {
                campaign_id,
                save_count: saves.len(),
                latest_save,
                unverified_save_count,
            });
        }
    }
    sort_saved_campaign_resumes(&mut resumes);
    Ok(resumes)
}

fn sort_saved_campaign_resumes(resumes: &mut [SavedCampaignResume]) {
    resumes.sort_by(|left, right| {
        right
            .latest_save
            .as_ref()
            .map(|save| save.modified_at)
            .cmp(&left.latest_save.as_ref().map(|save| save.modified_at))
            .then_with(|| left.campaign_id.cmp(&right.campaign_id))
    });
}

fn inspect_live_active_campaign_resumes(
    game_root: &Path,
    profile_dir: Option<&str>,
) -> Result<Vec<SavedCampaignResume>, String> {
    let Some(profile) = select_starcraft_profile(profile_dir)? else {
        return Ok(Vec::new());
    };
    let save_paths = collect_regular_files(&profile.path.join("Saves"))?
        .into_iter()
        .filter(|path| path.extension().and_then(|extension| extension.to_str()).is_some_and(|extension| extension.eq_ignore_ascii_case("SC2Save")))
        .filter(|path| is_user_loadable_campaign_save(&profile.path, path))
        .collect::<Vec<_>>();
    let mut resumes = Vec::new();
    for state in read_managed_states(game_root)? {
        let dependency_names = dependency_roots_from_managed_files(&state.files)
            .iter()
            .filter_map(|path| path.file_name().and_then(|name| name.to_str()))
            .map(|name| name.to_ascii_lowercase())
            .collect::<Vec<_>>();
        if dependency_names.is_empty() {
            continue;
        }
        let mut saves = save_paths.iter().filter_map(|path| {
            let details = inspect_save_details(path).ok()?;
            save_details_match_dependencies(&details, &dependency_names).then_some((path, details))
        }).collect::<Vec<_>>();
        saves.sort_by(|left, right| save_modified_at(right.0).cmp(&save_modified_at(left.0)));
        let save_count = saves.len();
        let latest_save = saves.first().map(|(path, details)| {
            let relative_path = path_string(path.strip_prefix(&profile.path).map_err(|_| "Live save path escaped the SC2 profile.")?);
            Ok::<SavedCampaignSave, String>(SavedCampaignSave {
                relative_path,
                modified_at: save_modified_at(path),
                map: details.maps.first().cloned(),
                details_available: true,
            })
        }).transpose()?;
        resumes.push(SavedCampaignResume {
            campaign_id: state.campaign_id,
            save_count,
            latest_save,
            unverified_save_count: 0,
        });
    }
    Ok(resumes)
}

fn read_campaign_profile_resume_manifest(
    path: &Path,
    campaign_id: &str,
) -> Result<CampaignProfileResumeManifest, String> {
    let text = fs::read_to_string(path).map_err(io_error)?;
    let manifest: CampaignProfileResumeManifest = serde_json::from_str(&text)
        .map_err(|_| "Campaign profile resume metadata is invalid.".to_string())?;
    if manifest.format != 1 || manifest.campaign_id != campaign_id {
        return Err("Campaign profile resume metadata does not match this campaign.".into());
    }
    validate_campaign_id(&manifest.campaign_id)?;
    let _ = safe_campaign_target(&manifest.target_path)?;
    if manifest.dependency_names.iter().any(|name| name.is_empty() || !name.to_ascii_lowercase().ends_with(".sc2mod")) {
        return Err("Campaign profile resume metadata has invalid dependencies.".into());
    }
    Ok(manifest)
}

fn save_modified_at(path: &Path) -> u64 {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

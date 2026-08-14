fn current_managed_campaign(root: &Path, target: &str) -> Result<Option<ManagedCampaignProfile>, String> {
    let Some(state) = read_state_for_target(root, target)? else {
        return Ok(None);
    };
    if !state.cleared_directories.iter().any(|directory| directory == target) {
        return Ok(None);
    }
    let dependencies = dependency_roots_from_managed_files(&state.files);
    Ok(Some(ManagedCampaignProfile {
        campaign_id: state.campaign_id,
        title: state.title,
        dependencies,
    }))
}

fn campaign_profile_spec(target: &str) -> Option<CampaignProfileSpec> {
    match target {
        "Maps/Campaign" => Some(CampaignProfileSpec {
            banks: &["WCampaign.SC2Bank", "WArchive.SC2Bank", "WArmy.SC2Bank", "WStory.SC2Bank", "WCampaignStats.SC2Bank"],
            save_marker: "liberty",
            progress_ids: &["WingsOfLiberty", "Liberty"],
        }),
        "Maps/Campaign/swarm" => Some(CampaignProfileSpec {
            banks: &["ZCampaign.SC2Bank", "ZArchive.SC2Bank", "ZArmy.SC2Bank", "ZStory.SC2Bank", "ZCampaignStats.SC2Bank"],
            save_marker: "swarm",
            progress_ids: &["HeartOfTheSwarm", "Swarm"],
        }),
        "Maps/Campaign/void" => Some(CampaignProfileSpec {
            banks: &["PCampaign.SC2Bank", "PArchive.SC2Bank", "PArmy.SC2Bank", "PStory.SC2Bank", "PCampaignStats.SC2Bank"],
            save_marker: "void",
            progress_ids: &["LegacyOfTheVoid", "Void"],
        }),
        "Maps/Campaign/nova" => Some(CampaignProfileSpec {
            banks: &["NCampaign.SC2Bank", "NArchive.SC2Bank", "NArmy.SC2Bank", "NStory.SC2Bank", "NCampaignStats.SC2Bank"],
            save_marker: "nova",
            progress_ids: &["NovaCovertOps", "Nova"],
        }),
        _ => None,
    }
}

fn save_filename_matches(name: &str, spec: &CampaignProfileSpec) -> bool {
    let name = name.to_ascii_lowercase();
    let marker = spec.save_marker.to_ascii_lowercase();
    if !name.contains(&marker) {
        return false;
    }
    // Prologue and epilogue saves are separate campaigns. A LotV mod must not
    // silently move them when it switches only the main Void campaign.
    if marker == "void" && (name.contains("prologue") || name.contains("epilogue")) {
        return false;
    }
    true
}

/// Whether a save is a player-loadable campaign checkpoint rather than one of
/// SC2's root-level campaign-state archives. We still snapshot the latter for
/// profile isolation, but must never present them as a "Load this save" hint.
fn is_user_loadable_campaign_save(profile_root: &Path, save_path: &Path) -> bool {
    let Ok(relative) = save_path.strip_prefix(profile_root) else {
        return false;
    };
    let relative = path_string(relative).to_ascii_lowercase();
    relative.starts_with("saves/campaign/") || relative.starts_with("saves/unsaved/campaign/")
}

fn campaign_slot_bank_paths(root: &Path, spec: &CampaignProfileSpec) -> Vec<PathBuf> {
    spec.banks
        .iter()
        .map(|name| root.join("Banks").join(name))
        .collect()
}

fn validate_profile_switch_roots(profile: &Path, store_base: &Path) -> Result<(), String> {
    let profile_type = fs::symlink_metadata(profile).map_err(io_error)?.file_type();
    if profile_type.is_symlink() || !profile_type.is_dir() {
        return Err("Selected StarCraft II profile must be a regular directory.".into());
    }
    for name in ["Banks", "Saves"] {
        let child = profile.join(name);
        if !child.exists() {
            continue;
        }
        let child_type = fs::symlink_metadata(&child).map_err(io_error)?.file_type();
        if child_type.is_symlink() || !child_type.is_dir() {
            return Err(format!("Selected profile {name} directory must not be a symlink."));
        }
    }
    if store_base.exists() {
        let store_type = fs::symlink_metadata(store_base).map_err(io_error)?.file_type();
        if store_type.is_symlink() || !store_type.is_dir() {
            return Err("CCM profile-store directory must be a regular directory, not a symlink.".into());
        }
        for name in ["Banks", "Saves"] {
            let child = store_base.join(name);
            if child.exists() && fs::symlink_metadata(&child).map_err(io_error)?.file_type().is_symlink() {
                return Err(format!("CCM profile-store {name} directory must not be a symlink."));
            }
        }
    }
    Ok(())
}

struct DiscoveredProfile {
    path: PathBuf,
    other_profiles: usize,
}

fn discover_starcraft_profile() -> Result<Option<DiscoveredProfile>, String> {
    let candidates = discover_starcraft_profiles()?;
    let Some(path) = candidates.first().cloned() else {
        return Ok(None);
    };
    Ok(Some(DiscoveredProfile {
        path,
        other_profiles: candidates.len().saturating_sub(1),
    }))
}

fn discover_starcraft_profiles() -> Result<Vec<PathBuf>, String> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    for accounts in standard_profile_locations() {
        if !accounts.is_dir() {
            continue;
        }
        for account in fs::read_dir(&accounts).map_err(io_error)?.flatten() {
            if !account.file_type().map_err(io_error)?.is_dir() {
                continue;
            }
            for profile in fs::read_dir(account.path()).map_err(io_error)?.flatten() {
                let path = profile.path();
                if !profile.file_type().map_err(io_error)?.is_dir()
                    || !is_starcraft_profile_directory(&path)
                    || !seen.insert(path.clone())
                {
                    continue;
                }
                candidates.push(path);
            }
        }
    }
    candidates.sort_by(|left, right| {
        profile_sort_key(right).cmp(&profile_sort_key(left)).then_with(|| left.cmp(right))
    });
    Ok(candidates)
}

fn is_starcraft_profile_directory(path: &Path) -> bool {
    path.join("Banks").is_dir() || path.join("Saves").is_dir() || path.join("CampaignProgress.xml").is_file()
}

fn profile_sort_key(path: &Path) -> (u8, u64) {
    let has_progress = u8::from(path.join("CampaignProgress.xml").is_file());
    let modified = [path.join("CampaignProgress.xml"), path.join("Banks"), path.join("Saves")]
        .iter()
        .filter_map(|candidate| fs::metadata(candidate).ok()?.modified().ok())
        .filter_map(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .max()
        .unwrap_or(0);
    (has_progress, modified)
}

fn standard_profile_locations() -> Vec<PathBuf> {
    let mut locations = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        locations.push(home.join("Library/Application Support/Blizzard/StarCraft II/Accounts"));
        locations.push(home.join(".local/share/Blizzard/StarCraft II/Accounts"));
    }
    if let Some(user_profile) = std::env::var_os("USERPROFILE") {
        let user_profile = PathBuf::from(user_profile);
        locations.push(user_profile.join("Documents/StarCraft II/Accounts"));
        locations.push(user_profile.join("AppData/Roaming/Blizzard/StarCraft II/Accounts"));
    }
    if let Some(app_data) = std::env::var_os("APPDATA") {
        locations.push(PathBuf::from(app_data).join("Blizzard/StarCraft II/Accounts"));
    }
    locations
}

fn display_profile_path(path: &Path) -> String {
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        if let Ok(relative) = path.strip_prefix(&home) {
            return format!("~/{}", path_string(relative));
        }
    }
    path_string(path)
}

fn profile_store_root(base: &Path, campaign_id: &str) -> PathBuf {
    base.join(campaign_id)
}

fn profile_store_base() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .map(|home| home.join(".ccm-reborn/profiles"))
        .ok_or("CCM could not determine the current user's home directory for profile snapshots.".into())
}

fn profile_store_base_for(profile: &Path) -> Result<PathBuf, String> {
    let canonical = fs::canonicalize(profile).map_err(io_error)?;
    let mut hasher = Sha256::new();
    hasher.update(path_string(&canonical).as_bytes());
    let key = format!("{:x}", hasher.finalize());
    let base = profile_store_base()?;
    validate_profile_store_chain(&base, &["accounts", &key])?;
    Ok(base.join("accounts").join(key))
}

fn validate_profile_store_chain(base: &Path, suffix: &[&str]) -> Result<(), String> {
    let home = base.parent().and_then(Path::parent).ok_or("CCM profile-store path is invalid.")?;
    let mut current = home.to_path_buf();
    for component in [".ccm-reborn", "profiles"].into_iter().chain(suffix.iter().copied()) {
        current.push(component);
        if !current.exists() {
            continue;
        }
        let file_type = fs::symlink_metadata(&current).map_err(io_error)?.file_type();
        if file_type.is_symlink() || !file_type.is_dir() {
            return Err(format!("CCM profile-store component {} must be a regular directory.", current.display()));
        }
    }
    Ok(())
}

/// Older builds stored snapshots only by campaign ID.  That is ambiguous once
/// two Battle.net account profiles use the app, so never silently import it
/// into whichever profile happens to be selected today.
fn reject_ambiguous_legacy_profile_store(campaign_id: &str, account_store: &Path) -> Result<(), String> {
    if campaign_id.is_empty() {
        return Ok(());
    }
    validate_campaign_id(campaign_id)?;
    let legacy = profile_store_base()?.join(campaign_id);
    if legacy.exists() {
        let imported = profile_store_root(account_store, campaign_id);
        if imported.is_dir()
            && !fs::symlink_metadata(&imported).map_err(io_error)?.file_type().is_symlink()
            && read_campaign_profile_resume_manifest(&imported.join("ccm-resume.json"), campaign_id).is_ok()
        {
            return Ok(());
        }
        return Err(format!(
            "Legacy CCM profile snapshot {} is not tied to an SC2 account. It was left untouched; migrate or remove it explicitly before switching this campaign.",
            legacy.display()
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileBackupEntry {
    path: PathBuf,
    backup: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileTransaction {
    entries: Vec<ProfileBackupEntry>,
}

impl ProfileTransaction {
    fn capture(paths: impl IntoIterator<Item = PathBuf>, staging: &Path) -> Result<Self, String> {
        let backup_root = staging.join("profile-rollback");
        fs::create_dir_all(&backup_root).map_err(io_error)?;
        let mut seen = HashSet::new();
        let mut entries = Vec::new();
        for path in paths {
            if !seen.insert(path.clone()) {
                continue;
            }
            let backup = if path.exists() {
                let file_type = fs::symlink_metadata(&path).map_err(io_error)?.file_type();
                if file_type.is_symlink() {
                    return Err(format!("Refusing to modify symlinked profile path {}.", path.display()));
                }
                if !file_type.is_file() {
                    return Err(format!("Profile path {} is not a regular file.", path.display()));
                }
                let backup = backup_root.join(format!("{:08}", entries.len()));
                copy_file(&path, &backup)?;
                Some(backup)
            } else {
                None
            };
            entries.push(ProfileBackupEntry { path, backup });
        }
        Ok(Self { entries })
    }

    fn rollback(&self) -> Result<(), String> {
        for entry in self.entries.iter().rev() {
            if let Some(backup) = &entry.backup {
                copy_file(backup, &entry.path)?;
            } else if entry.path.is_file() {
                fs::remove_file(&entry.path).map_err(io_error)?;
            } else if entry.path.exists() {
                return Err(format!("Cannot roll back non-file profile path {}.", entry.path.display()));
            }
        }
        Ok(())
    }
}

fn campaign_owned_save_paths(
    profile: &Path,
    spec: &CampaignProfileSpec,
    dependency_names: &[String],
    allow_unmanaged: bool,
) -> Result<Vec<PathBuf>, String> {
    let saves_root = profile.join("Saves");
    let mut owned = Vec::new();
    for save_path in collect_regular_files(&saves_root)? {
        if save_path.extension().and_then(|extension| extension.to_str()).map(|extension| extension.eq_ignore_ascii_case("SC2Save")) != Some(true) {
            continue;
        }
        let relative = save_path
            .strip_prefix(profile)
            .map_err(|_| "Save path escaped the StarCraft II profile.")?;
        let relative_text = path_string(relative);
        let filename_matches = relative
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| save_filename_matches(name, spec))
            .unwrap_or(false);
        let is_campaign_folder = relative_text.to_ascii_lowercase().starts_with("saves/campaign/")
            || relative_text.to_ascii_lowercase().starts_with("saves/unsaved/campaign/");
        if !filename_matches && !is_campaign_folder {
            continue;
        }
        // Slot-named saves (for example SwarmCampaignCompletedSave and
        // SwarmPublishArchive) are authoritative campaign state even when
        // `save.details` is absent or names a shared dependency.  They must
        // travel with the active slot; otherwise SC2 recreates its progress
        // after CCM reset the XML and primary bank.
        if filename_matches {
            owned.push(save_path);
            continue;
        }
        let details = match inspect_save_details(&save_path) {
            Ok(details) => details,
            Err(_) => continue,
        };
        let campaign_match = save_details_match_campaign(&details, spec);
        let dependency_match = if dependency_names.is_empty() {
            allow_unmanaged
        } else {
            save_details_match_dependencies(&details, dependency_names)
        };
        if campaign_match && dependency_match {
            owned.push(save_path);
        }
    }
    Ok(owned)
}

fn profile_store_file(store: &Path, profile: &Path, file: &Path) -> Result<PathBuf, String> {
    let relative = file
        .strip_prefix(profile)
        .map_err(|_| "Profile file escaped its root.")?;
    Ok(store.join(relative))
}

fn remove_profile_file(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let file_type = fs::symlink_metadata(path).map_err(io_error)?.file_type();
    if file_type.is_symlink() {
        return Err(format!("Refusing to remove symlinked profile file {}.", path.display()));
    }
    if !file_type.is_file() {
        return Err(format!("Profile path {} is not a regular file.", path.display()));
    }
    fs::remove_file(path).map_err(io_error)
}

fn write_text_atomic(path: &Path, text: &str) -> Result<(), String> {
    if path.exists() {
        let file_type = fs::symlink_metadata(path).map_err(io_error)?.file_type();
        if file_type.is_symlink() || !file_type.is_file() {
            return Err(format!("Refusing to replace non-regular profile file {}.", path.display()));
        }
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    let temporary = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    fs::write(&temporary, text).map_err(io_error)?;
    let file = File::open(&temporary).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    drop(file);
    atomic_replace(&temporary, path)
}

fn progress_node_matches(line: &str, spec: &CampaignProfileSpec) -> bool {
    spec.progress_ids.iter().any(|id| {
        line.contains(&format!("id=\"{id}\"")) || line.contains(&format!("id='{id}'"))
    })
}

fn select_starcraft_profile(profile_dir: Option<&str>) -> Result<Option<DiscoveredProfile>, String> {
    let Some(profile_dir) = profile_dir.map(str::trim).filter(|path| !path.is_empty()) else {
        return discover_starcraft_profile();
    };
    let path = PathBuf::from(profile_dir)
        .canonicalize()
        .map_err(|_| "Choose an existing StarCraft II account profile containing Banks, Saves, or CampaignProgress.xml.".to_string())?;
    if !path.is_dir() || !is_starcraft_profile_directory(&path) {
        return Err("Choose an existing StarCraft II account profile containing Banks, Saves, or CampaignProgress.xml.".into());
    }
    Ok(Some(DiscoveredProfile {
        path,
        other_profiles: 0,
    }))
}

fn replace_progress_node_line<F>(xml: &str, spec: &CampaignProfileSpec, mut transform: F) -> (String, bool)
where
    F: FnMut(&str) -> String,
{
    let mut changed = false;
    let mut result = String::with_capacity(xml.len());
    for chunk in xml.split_inclusive('\n') {
        let line = chunk.strip_suffix('\n').unwrap_or(chunk);
        let (content, newline) = line.strip_suffix('\r').map_or((line, ""), |line| (line, "\r"));
        if !changed && progress_node_matches(content, spec) {
            result.push_str(&transform(content));
            result.push_str(newline);
            if chunk.ends_with('\n') {
                result.push('\n');
            }
            changed = true;
        } else {
            result.push_str(chunk);
        }
    }
    (result, changed)
}

fn reset_progress_node(xml: &str, spec: &CampaignProfileSpec) -> (String, bool) {
    replace_progress_node_line(xml, spec, |line| {
        let mut value = line.to_string();
        for key in ["tutorialfinished", "campaignfinished"] {
            let marker = format!("{key}=");
            let Some(start) = value.find(&marker).map(|index| index + marker.len()) else {
                continue;
            };
            let remainder = &value[start..];
            let Some(quote) = remainder.chars().next().filter(|quote| *quote == '\"' || *quote == '\'') else {
                continue;
            };
            let Some(end) = remainder[1..].find(quote).map(|index| start + index + 1) else {
                continue;
            };
            value.replace_range(start + 1..end, "0");
        }
        value
    })
}

fn merge_progress_node(current: &str, stored: &str, spec: &CampaignProfileSpec) -> (String, bool) {
    let stored_line = stored
        .lines()
        .find(|line| progress_node_matches(line, spec));
    let Some(stored_line) = stored_line else {
        return (current.to_string(), false);
    };
    replace_progress_node_line(current, spec, |_| stored_line.to_string())
}

fn campaign_slot_name(target: &str) -> Result<&'static str, String> {
    campaign_slot_for_target(target).ok_or_else(|| format!("No campaign profile mapping exists for {target}."))
}

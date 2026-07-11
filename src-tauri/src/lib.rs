use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;
use zip::ZipArchive;

const MANAGER_DIRECTORY: &str = ".ccm-reborn";
const MAX_ARCHIVE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_METADATA_BYTES: u64 = 64 * 1024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRequest {
    campaign_id: String,
    title: String,
    archive_source: String,
    sha256: String,
    game_dir: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogDocument {
    format: u32,
    name: String,
    updated_at: String,
    campaigns: Vec<CatalogCampaign>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogCampaign {
    id: String,
    title: String,
    author: String,
    version: String,
    description: String,
    tags: Vec<String>,
    requirements: CampaignRequirements,
    package: CatalogPackage,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CampaignRequirements {
    campaign: String,
    platforms: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogPackage {
    url: Option<String>,
    path: Option<String>,
    sha256: String,
    size: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedCatalog {
    format: u32,
    name: String,
    updated_at: String,
    source_kind: String,
    campaigns: Vec<ResolvedCatalogCampaign>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedCatalogCampaign {
    id: String,
    title: String,
    author: String,
    version: String,
    description: String,
    tags: Vec<String>,
    requirements: CampaignRequirements,
    package: ResolvedCatalogPackage,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedCatalogPackage {
    source: String,
    sha256: String,
    size: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ManagedFile {
    destination: String,
    original_existed: bool,
    backup_path: Option<String>,
    installed_sha256: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ManagedState {
    format: u32,
    campaign_id: String,
    title: String,
    installed_at: u64,
    backup_dir: String,
    #[serde(default)]
    cleared_directories: Vec<String>,
    files: Vec<ManagedFile>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameInspection {
    exists: bool,
    path: String,
    looks_like_starcraft: bool,
    active_campaign: Option<ActiveCampaign>,
    active_campaigns: Vec<CurrentCampaign>,
    can_launch: bool,
    recovery_performed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveCampaign {
    id: String,
    title: String,
    files: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentCampaign {
    slot: String,
    campaign: String,
    title: String,
    author: String,
    version: String,
    is_modified: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchResult {
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameDirectoryCandidate {
    path: String,
    label: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KnownCampaign {
    title: String,
    author: String,
    version: String,
    package: KnownCampaignPackage,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KnownCampaignPackage {
    source: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallResult {
    campaign_id: String,
    title: String,
    files_installed: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    restored_files: usize,
    conflicts: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressFilePlan {
    relative_path: String,
    source: String,
    destination: String,
    kind: String,
    action: String,
    size: u64,
    sha256: String,
    detail: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChangePlan {
    source: String,
    destination: String,
    operation: String,
    kind: String,
    size: u64,
    sha256: Option<String>,
    detail: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressKeyChange {
    key: String,
    current_value: String,
    planned_value: String,
    action: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BankPlan {
    relative_path: String,
    source: String,
    destination: String,
    sections: usize,
    keys: usize,
    keys_changed_in_place: usize,
    note: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRunPlan {
    operation_id: String,
    campaign_id: String,
    title: String,
    game_directory: String,
    target_path: String,
    archive_size: u64,
    archive_sha256: String,
    package_files: usize,
    package_bytes: u64,
    campaign_files_to_clear: usize,
    campaign_bytes_to_clear: u64,
    dependency_roots: Vec<String>,
    dependency_files_to_replace: usize,
    files_to_backup: usize,
    profile_path: Option<String>,
    profile_store_path: Option<String>,
    profile_files_to_snapshot: usize,
    profile_bytes_to_snapshot: u64,
    progress_updates: usize,
    progress_files: Vec<ProgressFilePlan>,
    progress_keys: Vec<ProgressKeyChange>,
    bank_plans: Vec<BankPlan>,
    file_changes: Vec<FileChangePlan>,
    warnings: Vec<String>,
}

struct ProfilePlan {
    profile_path: Option<String>,
    profile_store_path: Option<String>,
    files: Vec<ProgressFilePlan>,
    progress_keys: Vec<ProgressKeyChange>,
    bank_plans: Vec<BankPlan>,
    warnings: Vec<String>,
}

struct ManagedCampaignProfile {
    campaign_id: String,
    title: String,
    dependencies: Vec<PathBuf>,
}

struct CampaignProfileSpec {
    bank: &'static str,
    save_marker: &'static str,
    progress_ids: &'static [&'static str],
}

struct SaveDetails {
    maps: Vec<String>,
    mods: Vec<String>,
    campaigns: Vec<String>,
}

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
            active_campaigns: Vec::new(),
            can_launch: false,
            recovery_performed: false,
        });
    }

    let recovery_performed = recover_interrupted_install(&root)?;
    let state = read_state(&root)?;
    let mut active_campaigns = inspect_current_campaigns(&root)?;
    identify_catalog_campaigns(&root, &mut active_campaigns, &known_campaigns.unwrap_or_default());
    Ok(GameInspection {
        exists: true,
        path: root.display().to_string(),
        looks_like_starcraft: looks_like_starcraft(&root),
        active_campaign: state.map(|state| ActiveCampaign {
            id: state.campaign_id,
            title: state.title,
            files: state.files.len(),
        }),
        active_campaigns,
        can_launch: can_launch_starcraft(&root),
        recovery_performed,
    })
}

#[tauri::command]
fn launch_current_campaign(game_dir: String) -> Result<LaunchResult, String> {
    let root = PathBuf::from(game_dir.trim());
    if !root.is_dir() {
        return Err("Choose an existing StarCraft II directory first.".into());
    }
    launch_starcraft(&root)?;
    Ok(LaunchResult {
        message: "StarCraft II launched. Choose Continue to resume the current campaign.".into(),
    })
}

#[tauri::command]
async fn plan_campaign_install(request: InstallRequest) -> Result<DryRunPlan, String> {
    tauri::async_runtime::spawn_blocking(move || plan_campaign_install_blocking(request))
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
    if request.archive_source.trim().starts_with("https://") {
        return Err("Dry-run currently requires a local package; remote download planning comes next.".into());
    }
    let archive_path = PathBuf::from(request.archive_source.trim());
    if !archive_path.is_file() {
        return Err("The local package declared in catalog.json was not found.".into());
    }

    let archive_size = fs::metadata(&archive_path).map_err(io_error)?.len();
    if archive_size > MAX_ARCHIVE_BYTES {
        return Err("Package is larger than the allowed archive size.".into());
    }
    let archive_sha256 = sha256_file(&archive_path)?;
    let expected_sha256 = normalize_sha256(&request.sha256)?;
    if archive_sha256 != expected_sha256 {
        return Err("The local archive does not match the catalog SHA-256. No files were changed.".into());
    }

    let package = read_ccm_package(&archive_path)?;
    let package_files = inspect_ccm_package_files(&archive_path, &package)?;
    let target = root.join(&package.target_path);
    let campaign_files = collect_regular_files(&target)?;
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
        .unwrap_or(request.campaign_id.as_str());
    let operation_id = Uuid::new_v4().to_string();
    let profile = plan_profile_transition(
        &root,
        &package.target_path,
        profile_campaign_id,
        current_managed.as_ref().map(|campaign| campaign.title.as_str()),
        &current_dependencies,
    )?;
    let backup_root = manager_root(&root)
        .join("backups")
        .join(format!("{}-{operation_id}", request.campaign_id));
    let mut file_changes = Vec::with_capacity(campaign_files.len() + dependency_files.len() + package_files.len());
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
    let profile_files_to_snapshot = profile.files.len();
    let profile_bytes_to_snapshot = profile.files.iter().map(|file| file.size).sum();
    let progress_updates = profile
        .files
        .iter()
        .filter(|file| file.kind == "campaign-progress")
        .count();
    let mut warnings = vec![
        "Dry-run only: no campaign, save, bank, or dependency files were changed.".into(),
        "Applying this plan later requires StarCraft II to be fully closed.".into(),
    ];
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
        progress_updates,
        progress_files: profile.files,
        progress_keys: profile.progress_keys,
        bank_plans: profile.bank_plans,
        file_changes,
        warnings,
    })
}

fn plan_profile_transition(
    root: &Path,
    target: &str,
    campaign_id: &str,
    current_title: Option<&str>,
    current_dependencies: &[PathBuf],
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

    let profile = match discover_starcraft_profile()? {
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
    let profile_store = profile_store_root(campaign_id);
    let profile_store_label = display_profile_path(&profile_store);
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
    let bank_path = profile.path.join("Banks").join(spec.bank);
    if bank_path.is_file() {
        let relative_path = path_string(bank_path.strip_prefix(&profile.path).map_err(|_| "Bank path escaped the profile.")?);
        let (sections, keys) = inspect_bank_shape(&bank_path)?;
        bank_plans.push(BankPlan {
            relative_path: relative_path.clone(),
            source: bank_path.display().to_string(),
            destination: profile_store.join(&relative_path).display().to_string(),
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
            &profile_store,
            &mut files,
        )?;
    }

    let progress_path = profile.path.join("CampaignProgress.xml");
    if progress_path.is_file() {
        let xml = fs::read_to_string(&progress_path).map_err(io_error)?;
        let matching_line = spec.progress_ids.iter().find_map(|id| {
            xml.lines()
                .find(|line| line.contains(&format!("id=\"{id}\"")) || line.contains(&format!("id='{id}'")))
                .map(str::trim)
        });
        if let Some(line) = matching_line {
            progress_keys = campaign_progress_key_changes(line);
            push_profile_file(
                &profile.path,
                &progress_path,
                "campaign-progress",
                "snapshot-and-reset-target-node",
                Some(format!("current node: {line}")),
                &profile_store,
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
        let (belongs, detail) = if filename_matches || is_campaign_folder {
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
                &profile_store,
                &mut files,
            )?;
        }
    }

    if dependency_names.is_empty() {
        warnings.push(
            "No managed dependency was found for the current campaign; individual campaign saves stay untouched until their mod identity is known.".into(),
        );
    }
    if files.is_empty() {
        warnings.push(format!(
            "No {} bank, save, or progress files were found in the selected profile.",
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

fn current_managed_campaign(root: &Path, target: &str) -> Result<Option<ManagedCampaignProfile>, String> {
    let Some(state) = read_state(root)? else {
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
            bank: "WCampaign.SC2Bank",
            save_marker: "liberty",
            progress_ids: &["WingsOfLiberty", "Liberty"],
        }),
        "Maps/Campaign/swarm" => Some(CampaignProfileSpec {
            bank: "ZCampaign.SC2Bank",
            save_marker: "swarm",
            progress_ids: &["HeartOfTheSwarm", "Swarm"],
        }),
        "Maps/Campaign/void" => Some(CampaignProfileSpec {
            bank: "PCampaign.SC2Bank",
            save_marker: "void",
            progress_ids: &["LegacyOfTheVoid", "Void"],
        }),
        "Maps/Campaign/nova" => Some(CampaignProfileSpec {
            bank: "NCampaign.SC2Bank",
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

struct DiscoveredProfile {
    path: PathBuf,
    other_profiles: usize,
}

fn discover_starcraft_profile() -> Result<Option<DiscoveredProfile>, String> {
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
    let Some(path) = candidates.first().cloned() else {
        return Ok(None);
    };
    Ok(Some(DiscoveredProfile {
        path,
        other_profiles: candidates.len().saturating_sub(1),
    }))
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

fn profile_store_root(campaign_id: &str) -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".ccm-reborn/profiles")
        .join(campaign_id)
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

fn push_profile_file(
    profile: &Path,
    path: &Path,
    kind: &str,
    action: &str,
    detail: Option<String>,
    destination_root: &Path,
    files: &mut Vec<ProgressFilePlan>,
) -> Result<(), String> {
    let metadata = fs::metadata(path).map_err(io_error)?;
    let relative_path = path_string(path.strip_prefix(profile).map_err(|_| "Profile path escaped its root.")?);
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

    let (catalog_json, source_kind, local_directory) = if source.starts_with("https://") {
        (
            download_text(source, 2 * 1024 * 1024)?,
            "remote".to_string(),
            None,
        )
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

    Ok(LoadedCatalog {
        format: catalog.format,
        name: catalog.name,
        updated_at: catalog.updated_at,
        source_kind,
        campaigns,
    })
}

#[tauri::command]
async fn install_campaign(request: InstallRequest) -> Result<InstallResult, String> {
    tauri::async_runtime::spawn_blocking(move || install_campaign_blocking(request))
        .await
        .map_err(|error| format!("Installation worker failed: {error}"))?
}

fn install_campaign_blocking(request: InstallRequest) -> Result<InstallResult, String> {
    validate_campaign_id(&request.campaign_id)?;
    if request.title.trim().is_empty() {
        return Err("Campaign title is required.".into());
    }

    let root = PathBuf::from(request.game_dir.trim());
    if !root.is_dir() {
        return Err("Choose an existing StarCraft II directory before installing.".into());
    }
    recover_interrupted_install(&root)?;
    let previous = restore_existing_campaign(&root)?;
    if !previous.conflicts.is_empty() {
        return Err("The active campaign has files changed outside CCM Reborn. Restore it manually before switching campaigns.".into());
    }

    let expected_hash = normalize_sha256(&request.sha256)?;
    let archive = acquire_archive(&request.archive_source)?;
    let result = install_archive(&root, &request, &archive.path, &expected_hash);
    if archive.is_temporary {
        let _ = fs::remove_file(&archive.path);
    }
    result
}

#[tauri::command]
async fn restore_original_campaigns(game_dir: String) -> Result<RestoreResult, String> {
    tauri::async_runtime::spawn_blocking(move || restore_original_campaigns_blocking(game_dir))
        .await
        .map_err(|error| format!("Restore worker failed: {error}"))?
}

fn restore_original_campaigns_blocking(game_dir: String) -> Result<RestoreResult, String> {
    let root = PathBuf::from(game_dir.trim());
    if !root.is_dir() {
        return Err("Choose an existing StarCraft II directory first.".into());
    }
    recover_interrupted_install(&root)?;
    restore_existing_campaign(&root)
}

fn install_archive(
    root: &Path,
    request: &InstallRequest,
    archive_path: &Path,
    expected_hash: &str,
) -> Result<InstallResult, String> {
    let actual_hash = sha256_file(archive_path)?;
    if actual_hash != expected_hash {
        return Err("The downloaded archive does not match the catalog SHA-256. Installation stopped.".into());
    }

    let package = read_ccm_package(archive_path)?;
    let managed_root = manager_root(root);
    let staging = managed_root.join("staging").join(Uuid::new_v4().to_string());
    fs::create_dir_all(&staging).map_err(io_error)?;

    let result = (|| {
        let staged_files = extract_ccm_package(archive_path, &package, &staging)?;
        let state = backup_campaign_directory(root, request, &package.target_path, &staged_files)?;

        // The journal makes a crash during a copy recover to vanilla on the next launch.
        write_json_atomic(&journal_path(root), &state)?;

        if let Err(error) = clear_directory(&root.join(&package.target_path)) {
            let _ = force_restore(root, &state);
            return Err(error);
        }
        if let Err(error) = clear_dependency_roots(root, &dependency_roots_from_staged(&staged_files)) {
            let _ = force_restore(root, &state);
            return Err(error);
        }
        for staged in &staged_files {
            let destination = root.join(&staged.destination);
            if let Err(error) = copy_file(&staged.path, &destination) {
                let _ = force_restore(root, &state);
                return Err(error);
            }
        }

        write_json_atomic(&state_path(root), &state)?;
        let _ = fs::remove_file(journal_path(root));

        Ok(InstallResult {
            campaign_id: state.campaign_id,
            title: state.title,
            files_installed: staged_files.len(),
        })
    })();

    let _ = fs::remove_dir_all(&staging);
    result
}

struct StagedFile {
    destination: String,
    path: PathBuf,
    sha256: String,
}

struct CcmPackage {
    content_prefix: String,
    target_path: String,
}

struct PackageFilePlan {
    source: String,
    destination: String,
    size: u64,
}

fn inspect_ccm_package_files(
    archive_path: &Path,
    package: &CcmPackage,
) -> Result<Vec<PackageFilePlan>, String> {
    let file = File::open(archive_path).map_err(io_error)?;
    let mut archive = ZipArchive::new(file).map_err(zip_error)?;
    let mut planned = Vec::new();
    for index in 0..archive.len() {
        let source = archive.by_index(index).map_err(zip_error)?;
        let source_name = source.name().to_string();
        if !source_name.starts_with(&package.content_prefix) || source.is_dir() {
            continue;
        }
        let relative = &source_name[package.content_prefix.len()..];
        if relative.is_empty() {
            continue;
        }
        if source.size() > MAX_FILE_BYTES {
            return Err(format!("Package file {source_name} is too large."));
        }
        let relative = safe_relative_path(relative)?;
        let destination = package_destination(&package.target_path, &relative)?;
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
    let mut metadata_indexes = Vec::new();
    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(zip_error)?;
        let name = file.name();
        if name.contains('\\') {
            return Err("CCM packages must use forward slashes in ZIP paths.".into());
        }
        if name == "metadata.txt" || name.ends_with("/metadata.txt") {
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
    let content_prefix = metadata_name
        .strip_suffix("metadata.txt")
        .ok_or("metadata.txt path is invalid.")?
        .to_string();
    Ok(CcmPackage {
        content_prefix,
        target_path,
    })
}

fn extract_ccm_package(
    archive_path: &Path,
    package: &CcmPackage,
    staging: &Path,
) -> Result<Vec<StagedFile>, String> {
    let file = File::open(archive_path).map_err(io_error)?;
    let mut archive = ZipArchive::new(file).map_err(zip_error)?;
    let mut staged_files = Vec::new();
    let mut total_size = 0_u64;

    for index in 0..archive.len() {
        let mut source = archive.by_index(index).map_err(zip_error)?;
        let source_name = source.name().to_string();
        if !source_name.starts_with(&package.content_prefix) {
            continue;
        }
        let relative = &source_name[package.content_prefix.len()..];
        if relative.is_empty() || source.is_dir() {
            continue;
        }
        if source.size() > MAX_FILE_BYTES {
            return Err(format!("Package file {source_name} is too large."));
        }
        total_size = total_size.saturating_add(source.size());
        if total_size > MAX_ARCHIVE_BYTES {
            return Err("Package expands beyond the allowed size.".into());
        }

        let relative = safe_relative_path(relative)?;
        let destination = package_destination(&package.target_path, &relative)?;
        let destination_string = path_string(&destination);
        let staged_path = staging.join(&destination);
        if let Some(parent) = staged_path.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        let mut output = File::create(&staged_path).map_err(io_error)?;
        let copied = io::copy(&mut source, &mut output).map_err(io_error)?;
        if copied != source.size() {
            return Err(format!("Could not fully extract {source_name}."));
        }
        output.sync_all().map_err(io_error)?;
        staged_files.push(StagedFile {
            destination: destination_string,
            sha256: sha256_file(&staged_path)?,
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
    let target = root.join(target_path);
    let mut files = Vec::new();
    let mut positions = HashMap::new();

    for original in collect_regular_files(&target)? {
        record_original_file(
            &original,
            root,
            &backup_root,
            &backup_dir,
            &mut files,
            &mut positions,
        )?;
    }

    for dependency_root in dependency_roots_from_staged(staged_files) {
        for original in collect_regular_files_or_file(&root.join(dependency_root))? {
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
        installed_at: unix_timestamp(),
        backup_dir,
        cleared_directories: vec![target_path.to_string()],
        files,
    })
}

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

fn restore_existing_campaign(root: &Path) -> Result<RestoreResult, String> {
    let Some(state) = read_state(root)? else {
        return Ok(RestoreResult {
            restored_files: 0,
            conflicts: Vec::new(),
        });
    };

    let conflicts = state
        .files
        .iter()
        .filter_map(|file| {
            let expected_hash = file.installed_sha256.as_ref()?;
            let destination = root.join(&file.destination);
            if !destination.exists() {
                return None;
            }
            match sha256_file(&destination) {
                Ok(hash) if hash == *expected_hash => None,
                Ok(_) | Err(_) => Some(file.destination.clone()),
            }
        })
        .collect::<Vec<_>>();
    if !conflicts.is_empty() {
        return Ok(RestoreResult {
            restored_files: 0,
            conflicts,
        });
    }

    force_restore(root, &state)?;
    let _ = fs::remove_file(state_path(root));
    let _ = fs::remove_dir_all(manager_root(root).join(&state.backup_dir));
    Ok(RestoreResult {
        restored_files: state.files.len(),
        conflicts: Vec::new(),
    })
}

fn force_restore(root: &Path, state: &ManagedState) -> Result<(), String> {
    for directory in &state.cleared_directories {
        clear_directory(&root.join(safe_campaign_target(directory)?))?;
    }
    clear_dependency_roots(root, &dependency_roots_from_managed_files(&state.files))?;
    for file in &state.files {
        let destination = root.join(&file.destination);
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

fn recover_interrupted_install(root: &Path) -> Result<bool, String> {
    let path = journal_path(root);
    if !path.is_file() {
        return Ok(false);
    }
    let transaction = read_state_file(&path)?;
    if let Some(state) = read_state(root)? {
        if state.backup_dir == transaction.backup_dir {
            let _ = fs::remove_file(path);
            return Ok(false);
        }
    }
    force_restore(root, &transaction)?;
    let _ = fs::remove_file(path);
    Ok(true)
}

fn read_state(root: &Path) -> Result<Option<ManagedState>, String> {
    let path = state_path(root);
    if !path.is_file() {
        return Ok(None);
    }
    read_state_file(&path).map(Some)
}

fn read_state_file(path: &Path) -> Result<ManagedState, String> {
    let json = fs::read_to_string(path).map_err(io_error)?;
    let state: ManagedState = serde_json::from_str(&json)
        .map_err(|_| "Managed installation state is corrupt; no files were changed.".to_string())?;
    if state.format != 1 {
        return Err("Managed installation state has an unsupported format.".into());
    }
    for file in &state.files {
        let destination = safe_relative_path(&file.destination)?;
        if destination.starts_with(MANAGER_DIRECTORY) {
            return Err("Managed installation state has an invalid destination.".into());
        }
        if let Some(backup) = &file.backup_path {
            let _ = safe_backup_path(backup)?;
        }
    }
    for directory in &state.cleared_directories {
        let _ = safe_campaign_target(directory)?;
    }
    Ok(state)
}

struct AcquiredArchive {
    path: PathBuf,
    is_temporary: bool,
}

fn acquire_archive(source: &str) -> Result<AcquiredArchive, String> {
    let source = source.trim();
    if !source.starts_with("https://") {
        let path = PathBuf::from(source);
        if !path.is_file() {
            return Err("The local package declared in catalog.json was not found.".into());
        }
        return Ok(AcquiredArchive {
            path,
            is_temporary: false,
        });
    }
    let path = download_archive(source)?;
    Ok(AcquiredArchive {
        path,
        is_temporary: true,
    })
}

fn download_archive(url: &str) -> Result<PathBuf, String> {
    let response = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
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

fn write_json_atomic(path: &Path, value: &ManagedState) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    let temporary = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    let json = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    let mut file = File::create(&temporary).map_err(io_error)?;
    file.write_all(&json).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    fs::rename(&temporary, path).map_err(io_error)
}

fn safe_relative_path(value: &str) -> Result<PathBuf, String> {
    if value.is_empty() || value.contains('\\') {
        return Err("Package paths must be non-empty relative paths using forward slashes.".into());
    }
    let path = Path::new(value);
    if !path.components().all(|component| matches!(component, Component::Normal(_))) {
        return Err("Package paths cannot be absolute or use . or .. segments.".into());
    }
    Ok(path.to_path_buf())
}

fn package_destination(target_path: &str, relative: &Path) -> Result<PathBuf, String> {
    if let Some(destination) = dependency_destination(relative) {
        return Ok(destination);
    }
    Ok(safe_relative_path(target_path)?.join(relative))
}

fn dependency_destination(relative: &Path) -> Option<PathBuf> {
    let components = relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect::<Vec<_>>();
    let dependency_index = components.iter().position(|name| {
        name.to_string_lossy()
            .to_ascii_lowercase()
            .ends_with(".sc2mod")
    })?;
    let mut destination = PathBuf::from("Mods");
    for component in &components[dependency_index..] {
        destination.push(component);
    }
    Some(destination)
}

fn dependency_root_from_destination(destination: &Path) -> Option<PathBuf> {
    let components = destination
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect::<Vec<_>>();
    if components.len() < 2 || !components[0].to_string_lossy().eq_ignore_ascii_case("mods") {
        return None;
    }
    let dependency_index = components.iter().skip(1).position(|name| {
        name.to_string_lossy()
            .to_ascii_lowercase()
            .ends_with(".sc2mod")
    })? + 1;
    let mut root = PathBuf::from("Mods");
    root.push(components[dependency_index]);
    Some(root)
}

fn dependency_roots_from_staged(staged_files: &[StagedFile]) -> Vec<PathBuf> {
    staged_files
        .iter()
        .filter_map(|file| dependency_root_from_destination(Path::new(&file.destination)))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect()
}

fn dependency_roots_from_managed_files(files: &[ManagedFile]) -> Vec<PathBuf> {
    files
        .iter()
        .filter_map(|file| dependency_root_from_destination(Path::new(&file.destination)))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect()
}

fn clear_dependency_roots(root: &Path, dependency_roots: &[PathBuf]) -> Result<(), String> {
    for dependency_root in dependency_roots {
        let path = root.join(dependency_root);
        if !path.exists() {
            continue;
        }
        let file_type = fs::symlink_metadata(&path).map_err(io_error)?.file_type();
        if file_type.is_symlink() {
            return Err(format!("Refusing to replace symlinked dependency {}.", path.display()));
        }
        if file_type.is_file() {
            fs::remove_file(&path).map_err(io_error)?;
        } else if file_type.is_dir() {
            fs::remove_dir_all(&path).map_err(io_error)?;
        } else {
            return Err(format!("Dependency {} is neither a file nor directory.", path.display()));
        }
    }
    Ok(())
}

fn safe_backup_path(value: &str) -> Result<PathBuf, String> {
    let path = safe_relative_path(value)?;
    if !path.starts_with("backups") {
        return Err("Managed installation state has an invalid backup path.".into());
    }
    Ok(path)
}

fn campaign_target_from_metadata(metadata: &str) -> Result<String, String> {
    let campaign = metadata
        .lines()
        .filter_map(|line| line.split_once('='))
        .find_map(|(key, value)| key.trim().eq_ignore_ascii_case("campaign").then_some(value.trim()))
        .ok_or("metadata.txt has no campaign= value.")?
        .to_ascii_lowercase();
    if campaign.contains("wings") || campaign.contains("liberty") || campaign.contains("wol") {
        return Ok("Maps/Campaign".into());
    }
    if campaign.contains("heart") || campaign.contains("swarm") || campaign.contains("hots") {
        return Ok("Maps/Campaign/swarm".into());
    }
    if campaign.contains("legacy") || campaign.contains("void") || campaign.contains("lotv") {
        return Ok("Maps/Campaign/void".into());
    }
    if campaign.contains("nova") || campaign.contains("covert") || campaign.contains("ops") || campaign.contains("nco") {
        return Ok("Maps/Campaign/nova".into());
    }
    Err("metadata.txt campaign= is not a campaign CCM Reborn understands.".into())
}

fn decode_ccm_metadata(bytes: &[u8]) -> Result<String, String> {
    if bytes.starts_with(&[0xff, 0xfe]) {
        return decode_utf16_metadata(&bytes[2..], true);
    }
    if bytes.starts_with(&[0xfe, 0xff]) {
        return decode_utf16_metadata(&bytes[2..], false);
    }
    String::from_utf8(bytes.to_vec())
        .map_err(|_| "metadata.txt must be valid UTF-8 or UTF-16 text.".into())
}

fn decode_utf16_metadata(bytes: &[u8], little_endian: bool) -> Result<String, String> {
    if bytes.len() % 2 != 0 {
        return Err("metadata.txt has an incomplete UTF-16 character.".into());
    }
    let units = bytes
        .chunks_exact(2)
        .map(|pair| {
            if little_endian {
                u16::from_le_bytes([pair[0], pair[1]])
            } else {
                u16::from_be_bytes([pair[0], pair[1]])
            }
        })
        .collect::<Vec<_>>();
    String::from_utf16(&units)
        .map_err(|_| "metadata.txt contains invalid UTF-16 text.".into())
}

fn safe_campaign_target(value: &str) -> Result<PathBuf, String> {
    let path = safe_relative_path(value)?;
    match value {
        "Maps/Campaign" | "Maps/Campaign/swarm" | "Maps/Campaign/void" | "Maps/Campaign/nova" => Ok(path),
        _ => Err("Managed installation state has an invalid campaign target.".into()),
    }
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn validate_campaign_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 80
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err("Campaign IDs may use only letters, numbers, dashes, and underscores.".into());
    }
    Ok(())
}

fn validate_catalog_campaign(campaign: &CatalogCampaign) -> Result<(), String> {
    validate_campaign_id(&campaign.id)?;
    if campaign.title.trim().is_empty()
        || campaign.author.trim().is_empty()
        || campaign.version.trim().is_empty()
        || campaign.description.trim().is_empty()
        || campaign.requirements.campaign.trim().is_empty()
        || campaign.requirements.platforms.is_empty()
    {
        return Err(format!("Campaign {} is missing required catalog fields.", campaign.id));
    }
    if campaign.package.size == 0 {
        return Err(format!("Campaign {} has no package size.", campaign.id));
    }
    normalize_sha256(&campaign.package.sha256)?;
    Ok(())
}

fn resolve_local_package(directory: &Path, declared_path: Option<&str>) -> Result<String, String> {
    let declared_path = declared_path.ok_or("Local campaigns need package.path.")?;
    let relative = Path::new(declared_path);
    if relative.is_absolute() || declared_path.contains('\\') {
        return Err("Local package.path must be a relative path using forward slashes.".into());
    }
    let candidate = directory.join(relative);
    let package = candidate
        .canonicalize()
        .map_err(|_| format!("Local package {} was not found.", declared_path))?;
    if !package.is_file() {
        return Err("Local package.path must point to a file.".into());
    }
    Ok(package.display().to_string())
}

fn looks_like_starcraft(root: &Path) -> bool {
    root.join("Support64").join("SC2Switcher_x64.exe").is_file()
        || root.join("StarCraft II.app").is_dir()
        || root.join("Versions").is_dir()
}

fn inspect_current_campaigns(root: &Path) -> Result<Vec<CurrentCampaign>, String> {
    let slots = [
        ("wings-of-liberty", "Wings of Liberty", "Maps/Campaign"),
        ("heart-of-the-swarm", "Heart of the Swarm", "Maps/Campaign/swarm"),
        ("legacy-of-the-void", "Legacy of the Void", "Maps/Campaign/void"),
        ("nova-covert-ops", "Nova Covert Ops", "Maps/Campaign/nova"),
    ];
    slots
        .into_iter()
        .map(|(slot, campaign, target)| {
            let metadata_path = root.join(target).join("metadata.txt");
            if !metadata_path.is_file() {
                return Ok(CurrentCampaign {
                    slot: slot.into(),
                    campaign: campaign.into(),
                    title: format!("Original or untracked {campaign}"),
                    author: "No CCM metadata.txt found".into(),
                    version: "Unknown".into(),
                    is_modified: false,
                });
            }
            let metadata_bytes = fs::read(&metadata_path).map_err(io_error)?;
            let metadata = decode_ccm_metadata(&metadata_bytes)?;
            Ok(CurrentCampaign {
                slot: slot.into(),
                campaign: campaign.into(),
                title: metadata_field(&metadata, "title").unwrap_or_else(|| format!("Custom {campaign}")),
                author: metadata_field(&metadata, "author").unwrap_or_else(|| "Unknown author".into()),
                version: metadata_field(&metadata, "version").unwrap_or_else(|| "Unknown version".into()),
                is_modified: true,
            })
        })
        .collect()
}

fn identify_catalog_campaigns(
    root: &Path,
    active_campaigns: &mut [CurrentCampaign],
    known_campaigns: &[KnownCampaign],
) {
    for known in known_campaigns {
        if known.package.source.starts_with("https://") {
            continue;
        }
        let archive_path = Path::new(&known.package.source);
        let Ok(package) = read_ccm_package(archive_path) else {
            continue;
        };
        let Some(slot) = campaign_slot_for_target(&package.target_path) else {
            continue;
        };
        let Some(current) = active_campaigns.iter_mut().find(|current| current.slot == slot) else {
            continue;
        };
        if current.is_modified || !ccm_package_matches_target(archive_path, &package, root) {
            continue;
        }
        current.title = known.title.clone();
        current.author = known.author.clone();
        current.version = known.version.clone();
        current.is_modified = true;
    }
}

fn campaign_slot_for_target(target: &str) -> Option<&'static str> {
    match target {
        "Maps/Campaign" => Some("wings-of-liberty"),
        "Maps/Campaign/swarm" => Some("heart-of-the-swarm"),
        "Maps/Campaign/void" => Some("legacy-of-the-void"),
        "Maps/Campaign/nova" => Some("nova-covert-ops"),
        _ => None,
    }
}

fn ccm_package_matches_target(archive_path: &Path, package: &CcmPackage, root: &Path) -> bool {
    let Ok(file) = File::open(archive_path) else {
        return false;
    };
    let Ok(mut archive) = ZipArchive::new(file) else {
        return false;
    };
    let mut verified_files = 0;
    for index in 0..archive.len() {
        let Ok(mut source) = archive.by_index(index) else {
            return false;
        };
        let source_name = source.name().to_string();
        if !source_name.starts_with(&package.content_prefix) || source.is_dir() {
            continue;
        }
        let relative = &source_name[package.content_prefix.len()..];
        if relative.is_empty() || relative == "metadata.txt" {
            continue;
        }
        let Ok(relative) = safe_relative_path(relative) else {
            return false;
        };
        let Ok(destination) = package_destination(&package.target_path, &relative) else {
            return false;
        };
        let target = root.join(destination);
        let Ok(target_metadata) = fs::metadata(&target) else {
            return false;
        };
        if !target_metadata.is_file() || target_metadata.len() != source.size() {
            return false;
        }
        let Ok(source_hash) = sha256_reader(&mut source) else {
            return false;
        };
        let Ok(target_hash) = sha256_file(&target) else {
            return false;
        };
        if source_hash != target_hash {
            return false;
        }
        verified_files += 1;
        if verified_files >= 2 {
            return true;
        }
    }
    verified_files > 0
}

fn metadata_field(metadata: &str, name: &str) -> Option<String> {
    metadata.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim().eq_ignore_ascii_case(name) && !value.trim().is_empty())
            .then_some(value.trim().to_string())
    })
}

fn standard_game_locations() -> Vec<PathBuf> {
    let mut locations = vec![
        PathBuf::from("/Applications/StarCraft II"),
        PathBuf::from("/Applications/StarCraft II.app"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        locations.extend([
            home.join("Applications/StarCraft II"),
            home.join("Applications/StarCraft II.app"),
            home.join("Library/Application Support/Blizzard/StarCraft II"),
        ]);
    }
    locations
}

fn find_game_root(selected: &Path) -> Option<PathBuf> {
    let candidates = [
        selected.to_path_buf(),
        selected.join("Contents/Resources"),
        selected.join("StarCraft II.app/Contents/Resources"),
        selected.join("StarCraft II"),
    ];
    for candidate in candidates {
        if is_game_root(&candidate) {
            return Some(candidate);
        }
        if let Some(version_root) = find_version_root(&candidate.join("Versions")) {
            return Some(version_root);
        }
    }
    None
}

fn find_version_root(versions: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(versions).ok()?;
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .find(|path| is_game_root(path))
}

fn is_game_root(path: &Path) -> bool {
    path.join("Maps/Campaign").is_dir()
        || path.join("SC2_x64.exe").is_file()
        || path.join("SC2Switcher_x64.exe").is_file()
        || (path.join("Maps").is_dir() && path.join("Mods").is_dir())
}

fn game_directory_candidate(path: PathBuf) -> GameDirectoryCandidate {
    GameDirectoryCandidate {
        label: path.display().to_string(),
        path: path.display().to_string(),
    }
}

#[cfg(target_os = "macos")]
fn can_launch_starcraft(_root: &Path) -> bool {
    true
}

#[cfg(target_os = "windows")]
fn can_launch_starcraft(root: &Path) -> bool {
    starcraft_executable(root).is_some()
}

#[cfg(target_os = "linux")]
fn can_launch_starcraft(root: &Path) -> bool {
    starcraft_executable(root).is_some()
}

#[cfg(target_os = "macos")]
fn launch_starcraft(_root: &Path) -> Result<(), String> {
    Command::new("open")
        .args(["-a", "StarCraft II"])
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not launch StarCraft II: {error}"))
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn launch_starcraft(root: &Path) -> Result<(), String> {
    let executable = starcraft_executable(root)
        .ok_or("Could not find the StarCraft II executable in the selected directory.")?;
    Command::new(executable)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not launch StarCraft II: {error}"))
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn starcraft_executable(root: &Path) -> Option<PathBuf> {
    ["SC2_x64.exe", "SC2Switcher_x64.exe", "SC2_x64"]
        .into_iter()
        .map(|name| root.join(name))
        .find(|candidate| candidate.is_file())
}

fn manager_root(root: &Path) -> PathBuf {
    root.join(MANAGER_DIRECTORY)
}

fn state_path(root: &Path) -> PathBuf {
    manager_root(root).join("state.json")
}

fn journal_path(root: &Path) -> PathBuf {
    manager_root(root).join("transaction.json")
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn io_error(error: io::Error) -> String {
    format!("Filesystem operation failed: {error}")
}

fn zip_error(error: zip::result::ZipError) -> String {
    format!("Could not read package archive: {error}")
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            inspect_game_directory,
            load_catalog,
            plan_campaign_install,
            install_campaign,
            restore_original_campaigns,
            launch_current_campaign,
            resolve_game_directory,
            detect_game_directories
        ])
        .run(tauri::generate_context!())
        .expect("error while running CCM Reborn");
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::{write::SimpleFileOptions, ZipWriter};

    #[test]
    fn installs_and_restores_a_standard_ccm_package() {
        let sandbox = std::env::temp_dir().join(format!("ccm-reborn-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&sandbox).unwrap();
        let archive_path = sandbox.join("hots-test.zip");
        let archive_file = File::create(&archive_path).unwrap();
        let mut archive = ZipWriter::new(archive_file);
        let options = SimpleFileOptions::default();
        archive
            .start_file("Test HoTS Mod/metadata.txt", options)
            .unwrap();
        archive
            .write_all(b"title=Test HoTS Mod\nauthor=Test\ncampaign=HotS\nversion=1\n")
            .unwrap();
        archive
            .start_file("Test HoTS Mod/zchar01.SC2Map", options)
            .unwrap();
        archive.write_all(b"custom mission").unwrap();
        archive
            .start_file("Test HoTS Mod/TestDependency.SC2Mod", options)
            .unwrap();
        archive.write_all(b"custom dependency").unwrap();
        archive.finish().unwrap();

        let game_dir = sandbox.join("game");
        let campaign_dir = game_dir.join("Maps/Campaign/swarm");
        fs::create_dir_all(&campaign_dir).unwrap();
        fs::write(campaign_dir.join("zchar01.SC2Map"), b"vanilla mission").unwrap();
        fs::write(campaign_dir.join("obsolete.SC2Map"), b"vanilla obsolete").unwrap();
        fs::create_dir_all(game_dir.join("Mods")).unwrap();
        fs::write(game_dir.join("Mods/TestDependency.SC2Mod"), b"old dependency").unwrap();

        let request = InstallRequest {
            campaign_id: "test-hots".into(),
            title: "Test HoTS Mod".into(),
            archive_source: archive_path.display().to_string(),
            sha256: sha256_file(&archive_path).unwrap(),
            game_dir: game_dir.display().to_string(),
        };
        let result = install_archive(&game_dir, &request, &archive_path, &request.sha256).unwrap();
        assert_eq!(result.files_installed, 3);
        assert_eq!(fs::read(campaign_dir.join("zchar01.SC2Map")).unwrap(), b"custom mission");
        assert!(!campaign_dir.join("obsolete.SC2Map").exists());
        assert!(!campaign_dir.join("TestDependency.SC2Mod").exists());
        assert_eq!(fs::read(game_dir.join("Mods/TestDependency.SC2Mod")).unwrap(), b"custom dependency");

        let restored = restore_existing_campaign(&game_dir).unwrap();
        assert_eq!(restored.conflicts.len(), 0);
        assert_eq!(fs::read(campaign_dir.join("zchar01.SC2Map")).unwrap(), b"vanilla mission");
        assert_eq!(fs::read(campaign_dir.join("obsolete.SC2Map")).unwrap(), b"vanilla obsolete");
        assert_eq!(fs::read(game_dir.join("Mods/TestDependency.SC2Mod")).unwrap(), b"old dependency");
        let _ = fs::remove_dir_all(sandbox);
    }

    #[test]
    fn dry_run_reports_scope_and_does_not_change_fixture_profile() {
        let sandbox = std::env::temp_dir().join(format!("ccm-reborn-dry-run-{}", Uuid::new_v4()));
        fs::create_dir_all(&sandbox).unwrap();
        let archive_path = sandbox.join("hots-fixture.zip");
        let archive_file = File::create(&archive_path).unwrap();
        let mut archive = ZipWriter::new(archive_file);
        let options = SimpleFileOptions::default();
        archive
            .start_file("Fixture HoTS Mod/metadata.txt", options)
            .unwrap();
        archive
            .write_all(b"title=Fixture HoTS Mod\nauthor=Test\ncampaign=HotS\nversion=1\n")
            .unwrap();
        archive
            .start_file("Fixture HoTS Mod/zchar01.SC2Map", options)
            .unwrap();
        archive.write_all(b"new mission").unwrap();
        archive
            .start_file("Fixture HoTS Mod/FixtureDependency.SC2Mod", options)
            .unwrap();
        archive.write_all(b"new dependency").unwrap();
        archive.finish().unwrap();

        let game_dir = sandbox.join("game");
        let swarm = game_dir.join("Maps/Campaign/swarm");
        let void = game_dir.join("Maps/Campaign/void");
        fs::create_dir_all(&swarm).unwrap();
        fs::create_dir_all(&void).unwrap();
        fs::write(swarm.join("zchar01.SC2Map"), b"old mission").unwrap();
        fs::write(swarm.join("obsolete.SC2Map"), b"old file").unwrap();
        fs::write(void.join("paiur01.SC2Map"), b"untouched LotV").unwrap();
        fs::create_dir_all(game_dir.join("Mods")).unwrap();
        fs::write(game_dir.join("Mods/FixtureDependency.SC2Mod"), b"old dependency").unwrap();
        let before = snapshot_files(&game_dir);

        let request = InstallRequest {
            campaign_id: "fixture-hots".into(),
            title: "Fixture HoTS Mod".into(),
            archive_source: archive_path.display().to_string(),
            sha256: sha256_file(&archive_path).unwrap(),
            game_dir: game_dir.display().to_string(),
        };
        let plan = plan_campaign_install_blocking(request).unwrap();

        assert_eq!(plan.package_files, 3);
        assert_eq!(plan.campaign_files_to_clear, 2);
        assert_eq!(plan.dependency_files_to_replace, 1);
        assert_eq!(plan.files_to_backup, 3);
        assert_eq!(plan.file_changes.len(), 6);
        assert!(plan.file_changes.iter().any(|change| {
            change.source.ends_with("Maps/Campaign/swarm/zchar01.SC2Map")
                && change.destination.contains("Maps/Campaign/swarm/zchar01.SC2Map")
        }));
        assert!(!plan.file_changes.iter().any(|change| {
            change.source.ends_with("Maps/Campaign/void/paiur01.SC2Map")
        }));
        assert!(plan.warnings.iter().any(|warning| warning.starts_with("Dry-run only")));
        assert_eq!(before, snapshot_files(&game_dir));
        let _ = fs::remove_dir_all(sandbox);
    }

    #[test]
    fn reads_utf16le_ccm_metadata() {
        let text = "title=Real Scale WoL\r\nauthor=Test\r\ncampaign=WoL\r\nversion=2.8\r\n";
        let mut bytes = vec![0xff, 0xfe];
        for code_unit in text.encode_utf16() {
            bytes.extend(code_unit.to_le_bytes());
        }

        let decoded = decode_ccm_metadata(&bytes).unwrap();
        assert_eq!(metadata_field(&decoded, "title"), Some("Real Scale WoL".into()));
        assert_eq!(campaign_target_from_metadata(&decoded).unwrap(), "Maps/Campaign");
    }

    #[test]
    fn extracts_campaign_paths_from_mpq_save_details() {
        let details = b"Campaign/Swarm/ZChar02.SC2Map\t&Mods/VioletsHoTSReworkMod.SC2Mod\0Campaigns/Swarm.SC2Campaign";
        assert_eq!(
            extract_archive_paths(details, "Campaign/", ".SC2Map"),
            vec!["Campaign/Swarm/ZChar02.SC2Map"]
        );
        assert_eq!(
            extract_archive_paths(details, "Mods/", ".SC2Mod"),
            vec!["Mods/VioletsHoTSReworkMod.SC2Mod"]
        );
        assert!(save_details_match_campaign(
            &SaveDetails {
                maps: vec!["Campaign/Swarm/ZChar02.SC2Map".into()],
                mods: vec![],
                campaigns: vec![],
            },
            &campaign_profile_spec("Maps/Campaign/swarm").unwrap()
        ));
    }

    #[test]
    fn save_filter_is_scoped_to_the_target_campaign_and_mod_identity() {
        let spec = campaign_profile_spec("Maps/Campaign/swarm").unwrap();
        assert!(save_filename_matches("SwarmCampaignSave.SC2Save", &spec));
        let lotv = campaign_profile_spec("Maps/Campaign/void").unwrap();
        assert!(!save_filename_matches("VoidPrologueCampaignSave.SC2Save", &lotv));
        assert!(save_details_match_dependencies(
            &SaveDetails {
                maps: vec![],
                mods: vec!["Mods/YuriCampaign.SC2Mod".into()],
                campaigns: vec![],
            },
            &["yuricampaign.sc2mod".into()]
        ));
        assert!(!save_details_match_dependencies(
            &SaveDetails {
                maps: vec![],
                mods: vec!["Mods/Unrelated.SC2Mod".into()],
                campaigns: vec![],
            },
            &["yuricampaign.sc2mod".into()]
        ));
    }

    fn snapshot_files(root: &Path) -> HashMap<String, Vec<u8>> {
        collect_regular_files(root)
            .unwrap()
            .into_iter()
            .map(|path| {
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                (relative, fs::read(path).unwrap())
            })
            .collect()
    }
}

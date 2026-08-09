///
/// Keeping the planner behind this wrapper means the CLI and the Tauri command
/// execute the exact same validation and dry-run code. The wrapper returns a
/// JSON value instead of exposing internal plan structs as public API.
pub fn cli_plan_json(
    campaign_id: String,
    title: String,
    archive_source: String,
    sha256: String,
    game_dir: String,
    profile_dir: Option<String>,
) -> Result<serde_json::Value, String> {
    let request = InstallRequest {
        campaign_id,
        title,
        author: String::new(),
        version: String::new(),
        profile_dir,
        archive_source,
        sha256,
        package_size: None,
        game_dir,
    };
    let plan = plan_campaign_install_blocking(request)?;
    let mut value = serde_json::to_value(plan)
        .map_err(|error| format!("Could not serialize dry-run plan: {error}"))?;
    if let serde_json::Value::Object(fields) = &mut value {
        fields.insert("schemaVersion".into(), serde_json::Value::from(1_u32));
    }
    Ok(value)
}

/// Explicit mutating CLI entry point. The caller must provide the same
/// package SHA used by the catalog; all safety checks in the Tauri command are
/// shared here rather than reimplemented in the CLI.
pub fn cli_install_json(
    campaign_id: String,
    title: String,
    author: String,
    version: String,
    archive_source: String,
    sha256: String,
    game_dir: String,
    profile_dir: Option<String>,
) -> Result<serde_json::Value, String> {
    let request = InstallRequest {
        campaign_id,
        title,
        author,
        version,
        profile_dir,
        archive_source,
        sha256,
        package_size: None,
        game_dir,
    };
    let result = install_campaign_blocking(request)?;
    serde_json::to_value(result).map_err(|error| format!("Could not serialize install result: {error}"))
}

pub fn cli_restore_json(game_dir: String, profile_dir: Option<String>, target_path: String) -> Result<serde_json::Value, String> {
    let result = restore_original_campaigns_blocking(game_dir, profile_dir, target_path)?;
    serde_json::to_value(result).map_err(|error| format!("Could not serialize restore result: {error}"))
}

/// Read the exact package ownership ledger for one campaign branch. This is
/// intentionally read-only and requires an explicit game directory/target.
pub fn cli_installed_manifest_json(game_dir: &Path, target_path: &str) -> Result<serde_json::Value, String> {
    let path = installed_manifest_path(game_dir, target_path)?;
    if !path.is_file() {
        return Err(format!("No installed manifest exists for {target_path}."));
    }
    let manifest = profile_core::read_installed_manifest(&path)?;
    let mut value = serde_json::to_value(manifest).map_err(|error| error.to_string())?;
    if let serde_json::Value::Object(fields) = &mut value {
        fields.insert("manifestPath".into(), serde_json::Value::String(path.display().to_string()));
    }
    Ok(value)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CliFixtureFile {
    relative_path: String,
    kind: String,
    size: u64,
    sha256: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CliFixtureSummary {
    schema_version: u32,
    root: String,
    file_count: usize,
    bytes: u64,
    files: Vec<CliFixtureFile>,
}

/// Produce a deterministic, read-only inventory for a fixture directory.
///
/// The command deliberately accepts an explicit path and never performs game
/// directory discovery. This is useful for checking sanitized fixtures in CI
/// without accidentally reading or changing a user's StarCraft II profile.
pub fn cli_fixture_summary_json(root: &Path) -> Result<serde_json::Value, String> {
    if !root.is_dir() {
        return Err(format!("Fixture root is not a directory: {}", root.display()));
    }
    let mut paths = collect_regular_files(root)?;
    paths.sort();
    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        let relative = path
            .strip_prefix(root)
            .map_err(|_| "Fixture file escaped its root.".to_string())?;
        let relative_path = path_string(relative);
        let metadata = fs::metadata(&path).map_err(io_error)?;
        files.push(CliFixtureFile {
            kind: fixture_file_kind(&relative_path),
            relative_path,
            size: metadata.len(),
            sha256: sha256_file(&path)?,
        });
    }
    let bytes = files.iter().map(|entry| entry.size).sum();
    let summary = CliFixtureSummary {
        schema_version: 1,
        root: root.display().to_string(),
        file_count: files.len(),
        bytes,
        files,
    };
    serde_json::to_value(summary).map_err(|error| format!("Could not serialize fixture summary: {error}"))
}

fn fixture_file_kind(relative_path: &str) -> String {
    let lower = relative_path.to_ascii_lowercase();
    if lower == "campaignprogress.xml" {
        "campaign-progress".into()
    } else if lower.starts_with("banks/") && lower.ends_with("campaign.sc2bank") {
        "campaign-bank".into()
    } else if lower.starts_with("saves/") && lower.ends_with(".sc2save") {
        "campaign-save".into()
    } else {
        "other".into()
    }
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

fn validate_archive_entry_count(entries: usize) -> Result<(), String> {
    if entries > MAX_ARCHIVE_ENTRIES {
        return Err("Package has too many ZIP entries.".into());
    }
    Ok(())
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
        let path = safe_game_path(root, dependency_root)?;
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
    {
        return Err(format!("Campaign {} is missing required catalog fields.", campaign.id));
    }
    if campaign.package.size == 0 {
        return Err(format!("Campaign {} has no package size.", campaign.id));
    }
    normalize_sha256(&campaign.package.sha256)?;
    Ok(())
}

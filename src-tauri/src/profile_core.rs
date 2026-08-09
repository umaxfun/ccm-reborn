//! Pure, filesystem-oriented profile primitives shared by Tauri and the CLI.
//! This module deliberately has no Tauri dependency and performs no implicit
//! discovery or mutation of a StarCraft II installation.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;

pub const MANIFEST_SCHEMA_VERSION: u32 = 1;
pub const INSTALLED_MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileIdentity {
    pub family_id: String,
    pub major_version: u32,
    pub campaign_id: String,
    pub profile_key: String,
}

impl ProfileIdentity {
    pub fn new(family_id: &str, major_version: u32, campaign_id: &str) -> Result<Self, String> {
        let family_id = validate_component(family_id, "familyId")?;
        let campaign_id = validate_component(campaign_id, "campaignId")?;
        if major_version == 0 {
            return Err("majorVersion must be greater than zero.".into());
        }
        Ok(Self {
            profile_key: format!("{family_id}@{major_version}"),
            family_id,
            major_version,
            campaign_id,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileFile {
    pub relative_path: String,
    pub size: u64,
    pub sha256: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressSnapshot {
    pub state: ProgressState,
    pub last_mission: Option<String>,
    pub last_successful_mission: Option<String>,
    pub last_map: Option<String>,
    pub mission_completed_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileManifest {
    pub schema_version: u32,
    pub identity: ProfileIdentity,
    pub branch: String,
    pub title: String,
    pub author: String,
    pub version: String,
    pub package_sha256: String,
    pub captured_at: u64,
    pub last_played_at: Option<u64>,
    pub last_played_source: Option<String>,
    pub files: Vec<ProfileFile>,
    pub progress: Option<ProgressSnapshot>,
}

/// The durable inventory of files written by one package installation.
///
/// This is intentionally separate from a profile manifest: a profile stores
/// user progress, while this manifest answers the safety-critical question
/// "which exact game files did CCM write?" during an update or rollback.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstalledFile {
    pub destination: String,
    pub source: String,
    pub size: u64,
    pub sha256: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstalledManifest {
    pub schema_version: u32,
    pub campaign_id: String,
    pub title: String,
    pub author: String,
    pub version: String,
    pub package_sha256: String,
    pub target_path: String,
    pub installed_at: u64,
    pub files: Vec<InstalledFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ProgressState {
    NotStarted,
    InProgress,
    Completed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CampaignProgressSummary {
    pub schema_version: u32,
    pub profile_key: String,
    pub family_id: String,
    pub major_version: u32,
    pub campaign_id: String,
    pub branch: String,
    pub title: String,
    pub author: String,
    pub version: String,
    pub is_current: bool,
    pub profile_present: bool,
    pub state: ProgressState,
    pub last_played_at: Option<u64>,
    pub last_played_source: Option<String>,
    pub last_mission: Option<String>,
    pub last_successful_mission: Option<String>,
    pub last_map: Option<String>,
    pub mission_completed_count: Option<u32>,
    pub save_count: usize,
    pub save_bytes: u64,
    pub warnings: Vec<String>,
}

pub fn sort_progress_summaries(summaries: &mut [CampaignProgressSummary]) {
    summaries.sort_by(|left, right| {
        progress_rank(right)
            .cmp(&progress_rank(left))
            .then_with(|| right.last_played_at.cmp(&left.last_played_at))
            .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
            .then_with(|| left.campaign_id.cmp(&right.campaign_id))
    });
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileDigest {
    pub relative_path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RoundTripReport {
    pub equal: bool,
    pub checked_files: usize,
    pub missing: Vec<String>,
    pub changed: Vec<String>,
}

pub fn snapshot_files(root: &Path, relative_paths: &[String]) -> Result<Vec<FileDigest>, String> {
    let mut result = Vec::with_capacity(relative_paths.len());
    for relative in relative_paths {
        let safe = safe_relative_path(relative)?;
        let path = root.join(&safe);
        let metadata = fs::metadata(&path).map_err(io_error)?;
        if !metadata.is_file() {
            return Err(format!("Expected regular file {}.", path.display()));
        }
        result.push(FileDigest {
            relative_path: path_string(&safe),
            size: metadata.len(),
            sha256: sha256_file(&path)?,
        });
    }
    result.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(result)
}

pub fn compare_snapshot(root: &Path, expected: &[FileDigest]) -> Result<RoundTripReport, String> {
    let mut missing = Vec::new();
    let mut changed = Vec::new();
    for digest in expected {
        let path = root.join(safe_relative_path(&digest.relative_path)?);
        if !path.is_file() {
            missing.push(digest.relative_path.clone());
            continue;
        }
        let metadata = fs::metadata(&path).map_err(io_error)?;
        let actual = sha256_file(&path)?;
        if metadata.len() != digest.size || actual != digest.sha256 {
            changed.push(digest.relative_path.clone());
        }
    }
    Ok(RoundTripReport {
        equal: missing.is_empty() && changed.is_empty(),
        checked_files: expected.len(),
        missing,
        changed,
    })
}

pub fn write_manifest_atomic(path: &Path, manifest: &ProfileManifest) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    let temporary = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    let bytes = serde_json::to_vec_pretty(manifest).map_err(|error| error.to_string())?;
    let mut file = File::create(&temporary).map_err(io_error)?;
    file.write_all(&bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    super::atomic_replace(&temporary, path)
}

pub fn read_manifest(path: &Path) -> Result<ProfileManifest, String> {
    let text = fs::read_to_string(path).map_err(io_error)?;
    let manifest: ProfileManifest = serde_json::from_str(&text)
        .map_err(|error| format!("Invalid profile manifest {}: {error}", path.display()))?;
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION {
        return Err(format!("Unsupported profile manifest schema {}.", manifest.schema_version));
    }
    Ok(manifest)
}

pub fn write_installed_manifest_atomic(path: &Path, manifest: &InstalledManifest) -> Result<(), String> {
    if manifest.schema_version != INSTALLED_MANIFEST_SCHEMA_VERSION {
        return Err(format!("Unsupported installed manifest schema {}.", manifest.schema_version));
    }
    if manifest.files.is_empty() {
        return Err("Installed manifest cannot be empty.".into());
    }
    for file in &manifest.files {
        safe_relative_path(&file.destination)?;
        if file.source.is_empty()
            || file.sha256.len() != 64
            || !file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("Installed manifest contains an invalid file entry.".into());
        }
    }
    write_json_atomic(path, manifest)
}

pub fn read_installed_manifest(path: &Path) -> Result<InstalledManifest, String> {
    let text = fs::read_to_string(path).map_err(io_error)?;
    let manifest: InstalledManifest = serde_json::from_str(&text)
        .map_err(|error| format!("Invalid installed manifest {}: {error}", path.display()))?;
    if manifest.schema_version != INSTALLED_MANIFEST_SCHEMA_VERSION {
        return Err(format!("Unsupported installed manifest schema {}.", manifest.schema_version));
    }
    if manifest.files.is_empty() {
        return Err("Installed manifest is empty.".into());
    }
    for file in &manifest.files {
        let destination = safe_relative_path(&file.destination)?;
        if destination.starts_with(".ccm-reborn") {
            return Err("Installed manifest cannot own CCM manager files.".into());
        }
        if file.sha256.len() != 64 || !file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("Installed manifest contains an invalid file hash.".into());
        }
    }
    Ok(manifest)
}

fn progress_rank(summary: &CampaignProgressSummary) -> u8 {
    match summary.state {
        ProgressState::InProgress | ProgressState::Completed => 1,
        ProgressState::NotStarted | ProgressState::Unknown => 0,
    }
}

fn validate_component(value: &str, field: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 100
        || !value.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(format!("{field} may use only letters, numbers, dots, dashes, and underscores."));
    }
    Ok(value.to_string())
}

fn safe_relative_path(value: &str) -> Result<PathBuf, String> {
    if value.is_empty() || value.contains('\\') {
        return Err("Path must be a non-empty relative path using forward slashes.".into());
    }
    let path = Path::new(value);
    if !path.components().all(|component| matches!(component, Component::Normal(_))) {
        return Err("Path cannot be absolute or contain . / .. segments.".into());
    }
    Ok(path.to_path_buf())
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(io_error)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(io_error)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

fn io_error(error: io::Error) -> String {
    error.to_string()
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    let temporary = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    let mut file = File::create(&temporary).map_err(io_error)?;
    file.write_all(&bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    super::atomic_replace(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(id: &str, title: &str, state: ProgressState, last: Option<u64>) -> CampaignProgressSummary {
        let identity = ProfileIdentity::new(id, 1, id).unwrap();
        CampaignProgressSummary {
            schema_version: 1,
            profile_key: identity.profile_key,
            family_id: identity.family_id,
            major_version: identity.major_version,
            campaign_id: identity.campaign_id,
            branch: "heart-of-the-swarm".into(),
            title: title.into(),
            author: "test".into(),
            version: "1.0".into(),
            is_current: false,
            profile_present: true,
            state,
            last_played_at: last,
            last_played_source: Some("manifest".into()),
            last_mission: None,
            last_successful_mission: None,
            last_map: None,
            mission_completed_count: None,
            save_count: 1,
            save_bytes: 1,
            warnings: Vec::new(),
        }
    }

    #[test]
    fn major_version_identity_is_stable_for_minor_updates() {
        let one = ProfileIdentity::new("nightmare", 1, "nightmare-v1").unwrap();
        let one_update = ProfileIdentity::new("nightmare", 1, "nightmare-v1").unwrap();
        let two = ProfileIdentity::new("nightmare", 2, "nightmare-v2").unwrap();
        assert_eq!(one.profile_key, one_update.profile_key);
        assert_ne!(one.profile_key, two.profile_key);
    }

    #[test]
    fn summaries_sort_progress_and_last_played_within_a_branch() {
        let mut values = vec![
            summary("old", "Old", ProgressState::InProgress, Some(10)),
            summary("new", "New", ProgressState::InProgress, Some(20)),
            summary("none", "None", ProgressState::NotStarted, None),
        ];
        sort_progress_summaries(&mut values);
        assert_eq!(values.iter().map(|value| value.campaign_id.as_str()).collect::<Vec<_>>(), ["new", "old", "none"]);
    }

    #[test]
    fn roundtrip_report_detects_mutation_and_restoration() {
        let root = std::env::temp_dir().join(format!("ccm-core-roundtrip-{}", uuid::Uuid::new_v4()));
        let source = root.join("source");
        let restored = root.join("restored");
        fs::create_dir_all(source.join("Saves")).unwrap();
        fs::create_dir_all(source.join("Banks")).unwrap();
        fs::write(source.join("Saves/Yuri.SC2Save"), b"yuri save").unwrap();
        fs::write(source.join("Banks/ZCampaign.SC2Bank"), b"yuri bank").unwrap();
        let paths = vec!["Saves/Yuri.SC2Save".into(), "Banks/ZCampaign.SC2Bank".into()];
        let expected = snapshot_files(&source, &paths).unwrap();
        for path in &paths {
            let source_path = source.join(path);
            let destination = restored.join(path);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::copy(source_path, destination).unwrap();
        }
        assert!(compare_snapshot(&restored, &expected).unwrap().equal);
        fs::write(restored.join("Saves/Yuri.SC2Save"), b"abathur save").unwrap();
        let changed = compare_snapshot(&restored, &expected).unwrap();
        assert!(!changed.equal);
        assert_eq!(changed.changed, vec!["Saves/Yuri.SC2Save"]);
        fs::write(restored.join("Saves/Yuri.SC2Save"), b"yuri save").unwrap();
        assert!(compare_snapshot(&restored, &expected).unwrap().equal);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn installed_manifest_roundtrips_exact_package_inventory() {
        let root = std::env::temp_dir().join(format!("ccm-installed-manifest-{}", uuid::Uuid::new_v4()));
        let path = root.join("installed/Maps_Campaign_swarm.json");
        let manifest = InstalledManifest {
            schema_version: INSTALLED_MANIFEST_SCHEMA_VERSION,
            campaign_id: "nightmare-v1".into(),
            title: "Nightmare".into(),
            author: "Test".into(),
            version: "1.35".into(),
            package_sha256: "a".repeat(64),
            target_path: "Maps/Campaign/swarm".into(),
            installed_at: 1,
            files: vec![InstalledFile {
                destination: "Maps/Campaign/swarm/zchar01.SC2Map".into(),
                source: "Nightmare/zchar01.SC2Map".into(),
                size: 3,
                sha256: "b".repeat(64),
                kind: "campaign file".into(),
            }],
        };
        write_installed_manifest_atomic(&path, &manifest).unwrap();
        assert_eq!(read_installed_manifest(&path).unwrap(), manifest);
        let _ = fs::remove_dir_all(root);
    }
}

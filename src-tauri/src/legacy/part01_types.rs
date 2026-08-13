use serde::{Deserialize, Serialize};
use fs2::FileExt;
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

#[path = "../profile_core.rs"]
pub mod profile_core;

use profile_core::{
    write_installed_manifest_atomic, InstalledFile, InstalledManifest,
    INSTALLED_MANIFEST_SCHEMA_VERSION,
};

const MANAGER_DIRECTORY: &str = ".ccm-reborn";
const MAX_ARCHIVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_METADATA_BYTES: u64 = 64 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 100_000;

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InstallRequest {
    campaign_id: String,
    title: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    profile_dir: Option<String>,
    archive_source: String,
    sha256: String,
    #[serde(default)]
    package_size: Option<u64>,
    game_dir: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MigrateLegacyProfileRequest {
    campaign_id: String,
    profile_dir: String,
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
    #[serde(default)]
    author: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    target_path: String,
    installed_at: u64,
    backup_dir: String,
    #[serde(default)]
    cleared_directories: Vec<String>,
    files: Vec<ManagedFile>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SharedDependencyBaseline {
    format: u32,
    dependency_root: String,
    original_existed: bool,
    original_was_file: bool,
    backup_dir: String,
}

impl ManagedState {
    fn target_path_or_first_clear(&self) -> String {
        if !self.target_path.is_empty() {
            return self.target_path.clone();
        }
        self.cleared_directories
            .first()
            .cloned()
            .unwrap_or_else(|| "Maps/Campaign".into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviousInstalledFile {
    destination: String,
    staged_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviousInstallSnapshot {
    state: ManagedState,
    backup_snapshot: PathBuf,
    files: Vec<PreviousInstalledFile>,
    manifest: Option<InstalledManifest>,
}

/// The durable intent record written before we touch either the StarCraft II
/// profile or the game directory.  The staging paths point to byte-for-byte
/// copies made by this operation, so recovery can put the *previous managed
/// campaign* back rather than guessing at vanilla files.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingInstallJournal {
    format: u32,
    profile_transaction: ProfileTransaction,
    /// Explicit roots make recovery reject a corrupt journal instead of ever
    /// writing a saved profile path outside SC2 or CCM's own profile store.
    #[serde(default)]
    profile_roots: Vec<PathBuf>,
    previous_install: Option<PreviousInstallSnapshot>,
    new_state: Option<ManagedState>,
    #[serde(default)]
    completed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameInspection {
    exists: bool,
    path: String,
    looks_like_starcraft: bool,
    active_campaign: Option<ActiveCampaign>,
    managed_campaigns: Vec<ActiveCampaign>,
    active_campaigns: Vec<CurrentCampaign>,
    can_launch: bool,
    recovery_performed: bool,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ActiveCampaign {
    id: String,
    title: String,
    slot: String,
    target_path: String,
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

/// A resumable save stored in CCM's per-campaign local snapshot.  This is
/// deliberately separate from SC2's cloud-backed "Continue campaign" state.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedCampaignResume {
    campaign_id: String,
    save_count: usize,
    latest_save: Option<SavedCampaignSave>,
    unverified_save_count: usize,
    last_played_at: Option<u64>,
    last_played_source: Option<String>,
    legacy_migration_pending: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedCampaignSave {
    relative_path: String,
    modified_at: u64,
    map: Option<String>,
    details_available: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CampaignProfileResumeManifest {
    format: u32,
    campaign_id: String,
    target_path: String,
    dependency_names: Vec<String>,
    captured_at: u64,
    #[serde(default)]
    legacy_migrated: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LegacyProfileMigration {
    campaign_id: String,
    files_copied: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameDirectoryCandidate {
    path: String,
    label: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StarcraftProfileCandidate {
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
    version: String,
    manifest_path: String,
    package_sha256: String,
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
    update_kind: String,
    previous_install_manifest: Option<String>,
    previous_install_campaign_id: Option<String>,
    previous_install_version: Option<String>,
    previous_install_sha256: Option<String>,
    previous_install_files: usize,
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
    profile_files_to_restore: usize,
    profile_bytes_to_restore: u64,
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
    banks: &'static [&'static str],
    save_marker: &'static str,
    progress_ids: &'static [&'static str],
}

struct SaveDetails {
    maps: Vec<String>,
    mods: Vec<String>,
    campaigns: Vec<String>,
}

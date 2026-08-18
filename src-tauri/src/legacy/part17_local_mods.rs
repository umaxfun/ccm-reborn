// Local mods: a player's own CCM package, added from disk and then managed
// next to the cloud catalogue entries.
//
// The archive is *copied* into CCM's own store when it is added. A reference
// to the player's file would break as soon as they cleaned their Downloads
// folder, and it would silently change identity when a mod author replaced
// the file in place under the same name.
//
// A record's `id` is assigned once and never recomputed. Campaign progress is
// keyed by campaign id (see `inspect_saved_campaign_resumes_at`), so changing
// an id would detach a player's play history and drop the mod back to the
// bottom of its branch as if it had never been played.

const LOCAL_MOD_STORE_FORMAT: u32 = 1;
const MAX_LOCAL_MODS: usize = 200;
const MAX_LOCAL_MOD_STORE_BYTES: u64 = 4 * 1024 * 1024;
/// `validate_campaign_id` allows 80 bytes; keep room for a collision suffix.
const MAX_LOCAL_MOD_ID_BYTES: usize = 70;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalModRecord {
    id: String,
    title: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    description: String,
    campaign: String,
    archive_file: String,
    sha256: String,
    size: u64,
    added_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalModStoreDocument {
    format: u32,
    mods: Vec<LocalModRecord>,
}

/// The stored record plus the absolute path of CCM's own copy. The path is
/// derived on read instead of being stored, so moving a home directory cannot
/// leave a stale path behind in the JSON.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalModEntry {
    #[serde(flatten)]
    record: LocalModRecord,
    archive_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPackageInspection {
    title: String,
    author: String,
    version: String,
    description: String,
    campaign: String,
    target_path: String,
    sha256: String,
    size: u64,
    files: usize,
    suggested_id: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalModOverrides {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

fn local_mod_root() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .map(|home| home.join(MANAGER_DIRECTORY).join("local"))
        .ok_or("CCM could not determine the current user's home directory for local mods.".into())
}

fn local_mod_store_path(root: &Path) -> PathBuf {
    root.join("local-mods.json")
}

fn local_mod_archive_directory(root: &Path) -> PathBuf {
    root.join("archives")
}

/// Creates the store directories, refusing to follow a symlink or to treat an
/// existing regular file as a directory.
fn prepare_local_mod_directories(root: &Path) -> Result<PathBuf, String> {
    let archives = local_mod_archive_directory(root);
    for path in [root, archives.as_path()] {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(format!(
                    "CCM local-mod component {} must be a regular directory.",
                    path.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir_all(path).map_err(io_error)?;
            }
            Err(error) => return Err(io_error(error)),
        }
    }
    Ok(archives)
}

fn read_local_mod_store_text(root: &Path) -> Result<Option<String>, String> {
    let path = local_mod_store_path(root);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error(error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("CCM local-mod list {} is not a regular file.", path.display()));
    }
    if metadata.len() > MAX_LOCAL_MOD_STORE_BYTES {
        return Err("CCM local-mod list is larger than the allowed size.".into());
    }
    fs::read_to_string(&path).map(Some).map_err(io_error)
}

/// Strict read. Used before every write: overwriting a list CCM cannot parse
/// would silently discard the player's other mods.
fn read_local_mods_strict(root: &Path) -> Result<Vec<LocalModRecord>, String> {
    let Some(text) = read_local_mod_store_text(root)? else {
        return Ok(Vec::new());
    };
    let document: LocalModStoreDocument = serde_json::from_str(&text)
        .map_err(|_| "CCM could not read its local-mod list; it is not valid JSON.".to_string())?;
    if document.format != LOCAL_MOD_STORE_FORMAT {
        return Err("CCM local-mod list uses an unsupported format.".into());
    }
    Ok(document.mods)
}

/// Tolerant read for display. A damaged list must not stop the application
/// from starting, so it degrades to "no local mods" and says so in the log.
fn read_local_mods_tolerant(root: &Path) -> Vec<LocalModRecord> {
    match read_local_mods_strict(root) {
        Ok(mods) => mods,
        Err(error) => {
            append_diagnostic_log(&format!("local-mod list unreadable: {}", redact_diagnostic_value(&error)));
            Vec::new()
        }
    }
}

fn write_local_mods(root: &Path, mods: &[LocalModRecord]) -> Result<(), String> {
    let document = LocalModStoreDocument {
        format: LOCAL_MOD_STORE_FORMAT,
        mods: mods.to_vec(),
    };
    let text = serde_json::to_string_pretty(&document)
        .map_err(|error| format!("Could not encode the local-mod list: {error}"))?;
    write_text_atomic(&local_mod_store_path(root), &text)
}

fn campaign_display_for_target(target_path: &str) -> Result<&'static str, String> {
    match target_path {
        "Maps/Campaign" => Ok("Wings of Liberty"),
        "Maps/Campaign/swarm" => Ok("Heart of the Swarm"),
        "Maps/Campaign/void" => Ok("Legacy of the Void"),
        "Maps/Campaign/nova" => Ok("Nova Covert Ops"),
        _ => Err("metadata.txt campaign= is not a campaign CCM Reborn understands.".into()),
    }
}

fn local_mod_slug(archive_file_name: &str) -> String {
    let stem = archive_file_name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(archive_file_name);
    let mut slug = String::new();
    for character in stem.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').to_string()
}

/// Mod archives are routinely named in non-Latin scripts, and
/// `validate_campaign_id` accepts only `[A-Za-z0-9_-]`. When a file name
/// carries no usable ASCII the hash provides a stable, collision-free stem.
fn suggested_local_mod_id(archive_file_name: &str, sha256: &str) -> String {
    let slug = local_mod_slug(archive_file_name);
    let slug = if slug.is_empty() {
        sha256.chars().take(12).collect::<String>()
    } else {
        slug
    };
    let mut id = format!("local-{slug}");
    id.truncate(MAX_LOCAL_MOD_ID_BYTES);
    id.trim_end_matches('-').to_string()
}

fn unique_local_mod_id(suggested: &str, mods: &[LocalModRecord]) -> Result<String, String> {
    let taken = |candidate: &str| mods.iter().any(|record| record.id == candidate);
    if !taken(suggested) {
        validate_campaign_id(suggested)?;
        return Ok(suggested.to_string());
    }
    for suffix in 2..=999 {
        let candidate = format!("{suggested}-{suffix}");
        if !taken(&candidate) {
            validate_campaign_id(&candidate)?;
            return Ok(candidate);
        }
    }
    Err("Too many local mods share that archive name.".into())
}

fn inspect_local_package_file(archive_path: &Path) -> Result<LocalPackageInspection, String> {
    let metadata = fs::symlink_metadata(archive_path)
        .map_err(|_| "That archive could not be opened. Choose a .zip file on this computer.".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Choose a regular .zip file, not a link or a folder.".into());
    }
    let size = metadata.len();
    if size == 0 {
        return Err("That archive is empty.".into());
    }
    if size > MAX_ARCHIVE_BYTES {
        return Err("Package is larger than the allowed size.".into());
    }
    let package = read_ccm_package(archive_path)?;
    let files = inspect_ccm_package_files(archive_path, &package)?;
    let campaign = campaign_display_for_target(&package.target_path)?;
    let sha256 = sha256_file(archive_path)?;
    let archive_file_name = archive_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let fallback_title = local_mod_slug(archive_file_name);
    Ok(LocalPackageInspection {
        title: metadata_field(&package.metadata_text, "title")
            .unwrap_or_else(|| if fallback_title.is_empty() { "Local campaign".into() } else { fallback_title }),
        author: metadata_field(&package.metadata_text, "author").unwrap_or_default(),
        version: metadata_field(&package.metadata_text, "version").unwrap_or_default(),
        description: metadata_field(&package.metadata_text, "desc")
            .or_else(|| metadata_field(&package.metadata_text, "description"))
            .unwrap_or_default(),
        campaign: campaign.to_string(),
        target_path: package.target_path,
        suggested_id: suggested_local_mod_id(archive_file_name, &sha256),
        sha256,
        size,
        files: files.len(),
    })
}

fn copy_local_archive(from: &Path, to: &Path) -> Result<(), String> {
    let temporary = to.with_extension(format!("{}.part", Uuid::new_v4()));
    let outcome = (|| -> Result<(), String> {
        let mut source = File::open(from).map_err(io_error)?;
        let mut target = File::create(&temporary).map_err(io_error)?;
        let copied = io::copy(&mut source, &mut target).map_err(io_error)?;
        if copied > MAX_ARCHIVE_BYTES {
            return Err("Package is larger than the allowed size.".into());
        }
        target.sync_all().map_err(io_error)
    })();
    if outcome.is_err() {
        let _ = fs::remove_file(&temporary);
        return outcome;
    }
    atomic_replace(&temporary, to)
}

fn override_or(value: Option<&String>, fallback: String) -> String {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
}

fn add_local_mod_at(
    root: &Path,
    archive_path: &Path,
    overrides: &LocalModOverrides,
) -> Result<LocalModEntry, String> {
    let inspection = inspect_local_package_file(archive_path)?;
    let mut mods = read_local_mods_strict(root)?;
    if mods.len() >= MAX_LOCAL_MODS {
        return Err("CCM already tracks the maximum number of local mods.".into());
    }
    let id = unique_local_mod_id(&inspection.suggested_id, &mods)?;
    let archives = prepare_local_mod_directories(root)?;
    let archive_file = format!("{id}.zip");
    let destination = archives.join(&archive_file);
    copy_local_archive(archive_path, &destination)?;
    // The player's file could have been replaced between hashing and copying.
    if sha256_file(&destination)? != inspection.sha256 {
        let _ = fs::remove_file(&destination);
        return Err("That archive changed while CCM was copying it. Nothing was added.".into());
    }
    let record = LocalModRecord {
        id,
        title: override_or(overrides.title.as_ref(), inspection.title),
        author: override_or(overrides.author.as_ref(), inspection.author),
        version: override_or(overrides.version.as_ref(), inspection.version),
        description: override_or(overrides.description.as_ref(), inspection.description),
        campaign: inspection.campaign,
        archive_file,
        sha256: inspection.sha256,
        size: inspection.size,
        added_at: unix_timestamp(),
    };
    mods.push(record.clone());
    if let Err(error) = write_local_mods(root, &mods) {
        // Never leave an archive CCM does not list.
        let _ = fs::remove_file(&destination);
        return Err(error);
    }
    Ok(local_mod_entry(root, record))
}

fn local_mod_entry(root: &Path, record: LocalModRecord) -> LocalModEntry {
    let archive_path = local_mod_archive_directory(root)
        .join(&record.archive_file)
        .display()
        .to_string();
    LocalModEntry { record, archive_path }
}

fn list_local_mods_at(root: &Path) -> Vec<LocalModEntry> {
    read_local_mods_tolerant(root)
        .into_iter()
        .map(|record| local_mod_entry(root, record))
        .collect()
}

/// Removes only CCM's own record and copy. The campaign installed in the game
/// directory is deliberately left alone.
fn remove_local_mod_at(root: &Path, id: &str) -> Result<bool, String> {
    let mut mods = read_local_mods_strict(root)?;
    let Some(position) = mods.iter().position(|record| record.id == id) else {
        return Ok(false);
    };
    let record = mods.remove(position);
    write_local_mods(root, &mods)?;
    let archive = local_mod_archive_directory(root).join(&record.archive_file);
    if fs::symlink_metadata(&archive).map(|metadata| metadata.is_file()).unwrap_or(false) {
        let _ = fs::remove_file(&archive);
    }
    Ok(true)
}

fn local_mod_store_root(store_dir: Option<String>) -> Result<PathBuf, String> {
    match store_dir.map(|path| path.trim().to_string()).filter(|path| !path.is_empty()) {
        Some(path) => Ok(PathBuf::from(path)),
        None => local_mod_root(),
    }
}

#[tauri::command]
async fn inspect_local_package(path: String) -> Result<LocalPackageInspection, String> {
    tauri::async_runtime::spawn_blocking(move || inspect_local_package_file(Path::new(path.trim())))
        .await
        .map_err(|error| format!("Local package worker failed: {error}"))?
}

#[tauri::command]
async fn add_local_mod(
    path: String,
    overrides: Option<LocalModOverrides>,
) -> Result<LocalModEntry, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = local_mod_root()?;
        add_local_mod_at(&root, Path::new(path.trim()), &overrides.unwrap_or_default())
    })
    .await
    .map_err(|error| format!("Local mod worker failed: {error}"))?
}

#[tauri::command]
async fn list_local_mods() -> Result<Vec<LocalModEntry>, String> {
    tauri::async_runtime::spawn_blocking(|| local_mod_root().map(|root| list_local_mods_at(&root)))
        .await
        .map_err(|error| format!("Local mod worker failed: {error}"))?
}

#[tauri::command]
async fn remove_local_mod(id: String) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = local_mod_root()?;
        remove_local_mod_at(&root, id.trim())
    })
    .await
    .map_err(|error| format!("Local mod worker failed: {error}"))?
}

pub fn cli_local_inspect_json(archive: String) -> Result<serde_json::Value, String> {
    let inspection = inspect_local_package_file(Path::new(archive.trim()))?;
    serde_json::to_value(inspection).map_err(|error| error.to_string())
}

pub fn cli_local_add_json(
    store_dir: Option<String>,
    archive: String,
    title: Option<String>,
    author: Option<String>,
    version: Option<String>,
) -> Result<serde_json::Value, String> {
    let root = local_mod_store_root(store_dir)?;
    let overrides = LocalModOverrides { title, author, version, description: None };
    let entry = add_local_mod_at(&root, Path::new(archive.trim()), &overrides)?;
    serde_json::to_value(entry).map_err(|error| error.to_string())
}

pub fn cli_local_list_json(store_dir: Option<String>) -> Result<serde_json::Value, String> {
    let root = local_mod_store_root(store_dir)?;
    serde_json::to_value(list_local_mods_at(&root)).map_err(|error| error.to_string())
}

pub fn cli_local_remove_json(store_dir: Option<String>, id: String) -> Result<serde_json::Value, String> {
    let root = local_mod_store_root(store_dir)?;
    let removed = remove_local_mod_at(&root, id.trim())?;
    Ok(serde_json::json!({ "id": id.trim(), "removed": removed }))
}

fn resolve_local_package(directory: &Path, declared_path: Option<&str>) -> Result<String, String> {
    let declared_path = declared_path.ok_or("Local campaigns need package.path.")?;
    let relative = safe_relative_path(declared_path)
        .map_err(|_| "Local package.path must be a contained relative path using forward slashes.".to_string())?;
    let catalog_root = directory.canonicalize().map_err(io_error)?;
    let candidate = catalog_root.join(relative);
    let package = candidate
        .canonicalize()
        .map_err(|_| format!("Local package {} was not found.", declared_path))?;
    if !package.is_file() {
        return Err("Local package.path must point to a file.".into());
    }
    if !package.starts_with(&catalog_root) {
        return Err("Local package.path must stay inside the catalog directory.".into());
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
        if source.is_dir() {
            continue;
        }
        let source_name = source.name().to_string();
        let Some(relative) = package_member_relative(&package, &source_name) else {
            continue;
        };
        if relative == "metadata.txt" {
            continue;
        }
        let Ok(relative) = safe_relative_path(relative) else {
            return false;
        };
        let Ok(destination) = package_destination(&package, &relative) else {
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
    #[cfg(target_os = "windows")]
    locations.extend(windows_game_locations(
        std::env::var_os("ProgramFiles").map(PathBuf::from),
        std::env::var_os("ProgramW6432").map(PathBuf::from),
        std::env::var_os("ProgramFiles(x86)").map(PathBuf::from),
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
        std::env::var_os("SystemDrive").map(PathBuf::from),
    ));
    locations
}

#[allow(dead_code)] // Called only by Windows production code and cross-platform regression tests.
fn windows_game_locations(
    program_files: Option<PathBuf>,
    program_w6432: Option<PathBuf>,
    program_files_x86: Option<PathBuf>,
    local_app_data: Option<PathBuf>,
    system_drive: Option<PathBuf>,
) -> Vec<PathBuf> {
    let mut locations = Vec::new();
    for base in [program_files, program_w6432, program_files_x86].into_iter().flatten() {
        let location = base.join("StarCraft II");
        if !locations.contains(&location) {
            locations.push(location);
        }
    }
    if let Some(local_app_data) = local_app_data {
        locations.extend([
            local_app_data.join("Blizzard/StarCraft II"),
            local_app_data.join("Programs/StarCraft II"),
        ]);
    }
    if let Some(system_drive) = system_drive {
        // `SystemDrive` normally is `C:`.  Joining a relative path directly
        // would produce `C:Games` on Windows (relative to the current
        // directory on that drive), so add the drive root explicitly.
        let drive = system_drive.to_string_lossy();
        let drive = drive.trim_end_matches(['\\', '/']);
        let separator = std::path::MAIN_SEPARATOR;
        let location = PathBuf::from(format!("{drive}{separator}Games{separator}StarCraft II"));
        if !locations.contains(&location) {
            locations.push(location);
        }
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

/// Desktop commands operate only on the exact game root selected by the UI.
/// The reusable core deliberately also supports fixture directories for the
/// shipped CLI/tests, so this boundary must live at the Tauri entry point.
fn require_desktop_game_root(value: &str) -> Result<PathBuf, String> {
    let selected = PathBuf::from(value.trim());
    let selected = selected
        .canonicalize()
        .map_err(|_| "Choose an existing StarCraft II directory first.".to_string())?;
    let resolved = find_game_root(&selected)
        .ok_or("The selected directory is not a StarCraft II game root.")?
        .canonicalize()
        .map_err(io_error)?;
    if resolved != selected {
        return Err("Choose the exact detected StarCraft II game directory, not a parent folder or .app bundle.".into());
    }
    if !has_desktop_starcraft_markers(&resolved) {
        return Err("The selected directory lacks the StarCraft II runtime markers required for a desktop install.".into());
    }
    Ok(resolved)
}

fn has_desktop_starcraft_markers(root: &Path) -> bool {
    root.join("SC2Data").is_dir()
        && (root.join("StarCraft II.app").is_dir()
            || root.join("Support").is_dir()
            || root.join("Support64").is_dir()
            || root.join("Versions").is_dir()
            || root.join("SC2_x64.exe").is_file()
            || root.join("SC2Switcher_x64.exe").is_file()
            || root.join("Support64/SC2_x64.exe").is_file()
            || root.join("Support64/SC2Switcher_x64.exe").is_file())
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
        || path.join("Support64/SC2_x64.exe").is_file()
        || path.join("Support64/SC2Switcher_x64.exe").is_file()
        || (path.join("Maps").is_dir() && path.join("Mods").is_dir())
}

fn game_directory_candidate(path: PathBuf) -> GameDirectoryCandidate {
    GameDirectoryCandidate {
        label: path.display().to_string(),
        path: path.display().to_string(),
    }
}

#[cfg(target_os = "macos")]
fn open_battle_net_desktop() -> Result<(), String> {
    Command::new("open")
        .args(["-b", "net.battle.app"])
        .status()
        .map_err(|_| "Could not open Battle.net. Start it yourself — everything else is ready.".to_string())
        .and_then(|status| {
            status.success()
                .then_some(())
                .ok_or_else(|| "Could not open Battle.net. Start it yourself — everything else is ready.".to_string())
        })
}

#[cfg(target_os = "windows")]
fn open_battle_net_desktop() -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let operation = "open\0".encode_utf16().collect::<Vec<_>>();
    let uri = "battlenet://\0".encode_utf16().collect::<Vec<_>>();
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            uri.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize <= 32 {
        return Err("Could not open Battle.net. Start it yourself — everything else is ready.".into());
    }
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_battle_net_desktop() -> Result<(), String> {
    Command::new("xdg-open")
        .arg("battlenet://")
        .status()
        .map_err(|_| "Could not open Battle.net. Start it yourself — everything else is ready.".to_string())
        .and_then(|status| {
            status.success()
                .then_some(())
                .ok_or_else(|| "Could not open Battle.net. Start it yourself — everything else is ready.".to_string())
        })
}

fn manager_root(root: &Path) -> PathBuf {
    root.join(MANAGER_DIRECTORY)
}

fn validate_manager_root(root: &Path) -> Result<(), String> {
    let root_type = fs::symlink_metadata(root).map_err(io_error)?.file_type();
    if root_type.is_symlink() || !root_type.is_dir() {
        return Err("Selected game directory is not a regular directory.".into());
    }
    let manager = manager_root(root);
    if !manager.exists() {
        return Ok(());
    }
    let manager_type = fs::symlink_metadata(&manager).map_err(io_error)?.file_type();
    if manager_type.is_symlink() || !manager_type.is_dir() {
        return Err("CCM's .ccm-reborn directory must be a regular directory, not a symlink.".into());
    }
    Ok(())
}

fn ensure_manager_root(root: &Path) -> Result<(), String> {
    validate_manager_root(root)?;
    fs::create_dir_all(manager_root(root)).map_err(io_error)?;
    validate_manager_root(root)
}

#[derive(Debug)]
struct OperationLock {
    _file: File,
}

fn operation_lock_is_held(error: &io::Error) -> bool {
    if error.kind() == io::ErrorKind::WouldBlock {
        return true;
    }
    #[cfg(windows)]
    {
        // Windows reports an overlapping byte-range lock as ERROR_LOCK_VIOLATION
        // (33), which Rust does not classify as WouldBlock.
        error.raw_os_error() == Some(33)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn acquire_operation_lock(root: &Path) -> Result<OperationLock, String> {
    ensure_manager_root(root)?;
    let path = manager_root(root).join("operation.lock");
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&path)
        .map_err(io_error)?;
    file.try_lock_exclusive().map_err(|error| {
        if operation_lock_is_held(&error) {
            "Another CCM operation is already running for this StarCraft II installation.".into()
        } else {
            io_error(error)
        }
    })?;
    Ok(OperationLock { _file: file })
}

fn state_path(root: &Path) -> PathBuf {
    manager_root(root).join("state.json")
}

fn state_path_for_target(root: &Path, target: &str) -> Result<PathBuf, String> {
    let target = safe_campaign_target(target)?;
    let name = path_string(&target).replace('/', "_");
    Ok(manager_root(root).join("states").join(format!("{name}.json")))
}

fn journal_path(root: &Path) -> PathBuf {
    manager_root(root).join("transaction.json")
}

fn pending_install_path(root: &Path) -> PathBuf {
    manager_root(root).join("pending-install.json")
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
            open_battle_net,
            inspect_saved_campaign_resumes,
            migrate_legacy_profile,
            resolve_game_directory,
            detect_game_directories,
            detect_starcraft_profiles,
            get_diagnostic_log_path,
            open_diagnostic_log_directory,
            inspect_local_package,
            add_local_mod,
            list_local_mods,
            remove_local_mod
        ])
        .run(tauri::generate_context!())
        .expect("error while running CCM Reborn");
}

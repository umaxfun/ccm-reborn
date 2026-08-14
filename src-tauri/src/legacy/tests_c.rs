#[test]
fn lotv_prologue_is_routed_to_its_separate_client_path_and_restored() {
    let sandbox = std::env::temp_dir().join(format!("ccm-lotv-prologue-{}", Uuid::new_v4()));
    let game_dir = sandbox.join("game");
    let archive_path = sandbox.join("lotv.zip");
    fs::create_dir_all(game_dir.join("Maps/Campaign/void")).unwrap();
    fs::create_dir_all(game_dir.join("Maps/Campaign/VoidPrologue")).unwrap();
    fs::write(game_dir.join("Maps/Campaign/void/paiur01.SC2Map"), b"vanilla-main").unwrap();
    fs::write(game_dir.join("Maps/Campaign/void/sc2epilogue01.SC2Map"), b"vanilla-epilogue").unwrap();
    fs::write(game_dir.join("Maps/Campaign/VoidPrologue/VoidPrologue01.SC2Map"), b"vanilla-prologue").unwrap();

    let mut archive = ZipWriter::new(File::create(&archive_path).unwrap());
    let options = SimpleFileOptions::default();
    archive.start_file("LotV/metadata.txt", options).unwrap();
    archive.write_all(b"title=LotV Test\nauthor=Test\ncampaign=LotV\nversion=1\n").unwrap();
    for (path, bytes) in [
        ("LotV/paiur01.SC2Map", b"custom-main" as &[u8]),
        ("LotV/sc2epilogue01.SC2Map", b"custom-epilogue"),
        ("LotV/voidprologue/VoidPrologue01.SC2Map", b"custom-prologue"),
    ] {
        archive.start_file(path, options).unwrap();
        archive.write_all(bytes).unwrap();
    }
    archive.finish().unwrap();

    let request = InstallRequest {
        campaign_id: "lotv-prologue-test".into(), title: "LotV Test".into(), author: "Test".into(), version: "1".into(),
        profile_dir: None, archive_source: archive_path.display().to_string(), sha256: sha256_file(&archive_path).unwrap(), package_size: None,
        game_dir: game_dir.display().to_string(),
    };
    let plan = plan_campaign_install_blocking(request.clone()).unwrap();
    assert!(plan.file_changes.iter().any(|change| change.destination.ends_with("Maps/Campaign/VoidPrologue/VoidPrologue01.SC2Map")));
    install_campaign_blocking(request).unwrap();
    assert_eq!(fs::read(game_dir.join("Maps/Campaign/void/paiur01.SC2Map")).unwrap(), b"custom-main");
    assert_eq!(fs::read(game_dir.join("Maps/Campaign/void/sc2epilogue01.SC2Map")).unwrap(), b"custom-epilogue");
    assert_eq!(fs::read(game_dir.join("Maps/Campaign/VoidPrologue/VoidPrologue01.SC2Map")).unwrap(), b"custom-prologue");
    assert!(!game_dir.join("Maps/Campaign/void/voidprologue/VoidPrologue01.SC2Map").exists());
    let state = read_state_for_target(&game_dir, "Maps/Campaign/void").unwrap().unwrap();
    assert_eq!(state.cleared_directories, vec!["Maps/Campaign/void", "Maps/Campaign/VoidPrologue"]);

    restore_original_campaigns_blocking(game_dir.display().to_string(), None, "Maps/Campaign/void".into()).unwrap();
    assert_eq!(fs::read(game_dir.join("Maps/Campaign/void/paiur01.SC2Map")).unwrap(), b"vanilla-main");
    assert_eq!(fs::read(game_dir.join("Maps/Campaign/void/sc2epilogue01.SC2Map")).unwrap(), b"vanilla-epilogue");
    assert_eq!(fs::read(game_dir.join("Maps/Campaign/VoidPrologue/VoidPrologue01.SC2Map")).unwrap(), b"vanilla-prologue");
    let _ = fs::remove_dir_all(sandbox);
}

#[test]
fn legacy_migration_prompt_excludes_campaigns_already_attached_to_this_account() {
    let sandbox = std::env::temp_dir().join(format!("ccm-legacy-prompt-{}", Uuid::new_v4()));
    let account_store = sandbox.join("account");
    fs::create_dir_all(account_store.join("already-attached")).unwrap();
    let resume = |campaign_id: &str| SavedCampaignResume {
        campaign_id: campaign_id.into(), save_count: 0, latest_save: None, unverified_save_count: 0,
        last_played_at: Some(1), last_played_source: Some("legacy-ccm-snapshot".into()), legacy_migration_pending: true,
    };
    let mut resumes = vec![SavedCampaignResume {
        legacy_migration_pending: false,
        ..resume("already-attached")
    }];
    merge_unattached_legacy_resumes(
        &mut resumes,
        vec![resume("already-attached"), resume("needs-migration")],
        &account_store,
    )
    .unwrap();
    assert_eq!(resumes.len(), 2);
    assert!(!resumes.iter().find(|resume| resume.campaign_id == "already-attached").unwrap().legacy_migration_pending);
    assert!(resumes.iter().find(|resume| resume.campaign_id == "needs-migration").unwrap().legacy_migration_pending);
    let _ = fs::remove_dir_all(sandbox);
}

#[test]
fn windows_support64_layout_is_detected() {
    let sandbox = std::env::temp_dir().join(format!("ccm-windows-layout-{}", Uuid::new_v4()));
    let game = sandbox.join("StarCraft II");
    fs::create_dir_all(game.join("SC2Data")).unwrap();
    fs::create_dir_all(game.join("Maps/Campaign")).unwrap();
    fs::create_dir_all(game.join("Support64")).unwrap();
    fs::write(game.join("Support64/SC2Switcher_x64.exe"), b"launcher").unwrap();

    assert_eq!(find_game_root(&game), Some(game.clone()));
    assert!(has_desktop_starcraft_markers(&game));
    let locations = windows_game_locations(
        Some(PathBuf::from("C:/Program Files")),
        Some(PathBuf::from("C:/Program Files")),
        Some(PathBuf::from("C:/Program Files (x86)")),
        Some(PathBuf::from("C:/Users/test/AppData/Local")),
        Some(PathBuf::from("C:")),
    );
    assert!(locations.contains(&PathBuf::from("C:/Program Files/StarCraft II")));
    assert!(locations.contains(&PathBuf::from("C:/Program Files (x86)/StarCraft II")));
    assert!(locations.contains(&PathBuf::from("C:/Users/test/AppData/Local/Blizzard/StarCraft II")));
    assert!(locations.contains(&PathBuf::from("C:/Games/StarCraft II")));
    let program_files_location = PathBuf::from("C:/Program Files/StarCraft II");
    assert_eq!(locations.iter().filter(|path| *path == &program_files_location).count(), 1);

    let _ = fs::remove_dir_all(sandbox);
}

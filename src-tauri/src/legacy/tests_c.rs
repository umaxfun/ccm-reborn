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

#[test]
fn install_over_a_map_previously_backed_up_as_a_folder_but_now_a_file_does_not_crash() {
    // Regression for the "Lings of Wiberty" crash: a previous mod shipped
    // `tvalerian02b.SC2Map` as an unpacked FOLDER (so CCM recorded its
    // originals as folder-form files), while the currently installed mod
    // shipped the same map name as a single FILE. Installing yet another
    // campaign walked the folder-form originals and hit
    // "Game path ancestor ... is not a directory". It must now succeed.
    let sandbox = std::env::temp_dir().join(format!("ccm-shape-mismatch-{}", Uuid::new_v4()));
    let game_dir = sandbox.join("game");
    let wol = game_dir.join("Maps/Campaign");
    // Pre-existing map shipped as an unpacked directory.
    fs::create_dir_all(wol.join("mission.SC2Map")).unwrap();
    fs::write(wol.join("mission.SC2Map/DocumentHeader"), b"vanilla-folder-map").unwrap();

    let make_archive = |path: &Path, name: &str, map: &str, bytes: &[u8]| {
        let file = File::create(path).unwrap();
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        archive.start_file(format!("{name}/metadata.txt"), options).unwrap();
        archive
            .write_all(format!("title={name}\nauthor=Test\ncampaign=WoL\nversion=1\n").as_bytes())
            .unwrap();
        archive.start_file(format!("{name}/{map}"), options).unwrap();
        archive.write_all(bytes).unwrap();
        archive.finish().unwrap();
    };
    let request = |archive: &Path, id: &str| InstallRequest {
        campaign_id: id.into(),
        title: id.into(),
        author: "Test".into(),
        version: "1.0".into(),
        profile_dir: None,
        archive_source: archive.display().to_string(),
        sha256: sha256_file(archive).unwrap(),
        package_size: None,
        game_dir: game_dir.display().to_string(),
    };

    // Mod A repacks the same map name as a single FILE. After this the managed
    // state holds folder-form originals AND a file-form install for
    // `mission.SC2Map` -- exactly the conflicting shapes.
    let a = sandbox.join("a.zip");
    make_archive(&a, "ModA", "mission.SC2Map", b"mod-a-file-map");
    install_campaign_blocking(request(&a, "mod-a")).unwrap();
    assert!(wol.join("mission.SC2Map").is_file(), "mod A installed the map as a file");

    // Installing another WoL campaign triggers the restore of mod A, which
    // walks the folder-form originals through safe_game_path. This is the step
    // that used to crash.
    let b = sandbox.join("b.zip");
    make_archive(&b, "ModB", "nightmare.SC2Map", b"mod-b-file-map");
    install_campaign_blocking(request(&b, "mod-b")).unwrap();

    assert!(wol.join("nightmare.SC2Map").is_file(), "mod B installed cleanly");
    assert!(!wol.join("mission.SC2Map").exists(), "mod A's map was cleared before mod B");
    let _ = fs::remove_dir_all(sandbox);
}

// --- Local mods (CCM-7) ----------------------------------------------------

fn write_local_fixture_archive(path: &Path, folder: &str, metadata: &str) {
    let mut archive = ZipWriter::new(File::create(path).unwrap());
    let options = SimpleFileOptions::default();
    archive.start_file(format!("{folder}/metadata.txt"), options).unwrap();
    archive.write_all(metadata.as_bytes()).unwrap();
    archive.start_file(format!("{folder}/zlab01.SC2Map"), options).unwrap();
    archive.write_all(b"local-mod-map").unwrap();
    archive.finish().unwrap();
}

#[test]
fn local_package_inspection_reads_metadata_and_refuses_broken_archives() {
    let sandbox = std::env::temp_dir().join(format!("ccm-local-inspect-{}", Uuid::new_v4()));
    fs::create_dir_all(&sandbox).unwrap();

    let good = sandbox.join("My HotS Mod.zip");
    write_local_fixture_archive(&good, "My HotS Mod", "title=My HotS Mod\nauthor=Kit\ncampaign=HotS\nversion=1.0.2\ndesc=Short text\n");
    let inspection = inspect_local_package_file(&good).unwrap();
    assert_eq!(inspection.title, "My HotS Mod");
    assert_eq!(inspection.author, "Kit");
    assert_eq!(inspection.version, "1.0.2");
    assert_eq!(inspection.description, "Short text");
    assert_eq!(inspection.campaign, "Heart of the Swarm");
    assert_eq!(inspection.target_path, "Maps/Campaign/swarm");
    // The count matches the install inventory, which copies metadata.txt too.
    assert_eq!(inspection.files, 2);
    assert_eq!(inspection.suggested_id, "local-my-hots-mod");

    // No metadata.txt at all.
    let empty = sandbox.join("empty.zip");
    let mut archive = ZipWriter::new(File::create(&empty).unwrap());
    archive.start_file("Mod/zlab01.SC2Map", SimpleFileOptions::default()).unwrap();
    archive.write_all(b"no-metadata").unwrap();
    archive.finish().unwrap();
    assert!(inspect_local_package_file(&empty).is_err());

    // Two metadata.txt files.
    let doubled = sandbox.join("doubled.zip");
    let mut archive = ZipWriter::new(File::create(&doubled).unwrap());
    let options = SimpleFileOptions::default();
    for folder in ["A", "B"] {
        archive.start_file(format!("{folder}/metadata.txt"), options).unwrap();
        archive.write_all(b"title=X\ncampaign=HotS\n").unwrap();
    }
    archive.finish().unwrap();
    assert!(inspect_local_package_file(&doubled).is_err());

    // A campaign value CCM cannot route.
    let unknown = sandbox.join("unknown.zip");
    write_local_fixture_archive(&unknown, "Unknown", "title=X\ncampaign=Brood War\n");
    assert!(inspect_local_package_file(&unknown).is_err());

    let _ = fs::remove_dir_all(sandbox);
}

#[test]
fn local_mod_ids_stay_stable_across_removal_title_edits_and_non_latin_names() {
    let sandbox = std::env::temp_dir().join(format!("ccm-local-id-{}", Uuid::new_v4()));
    let store = sandbox.join("store");
    fs::create_dir_all(&sandbox).unwrap();

    let archive = sandbox.join("Yuri of the Swarm.zip");
    write_local_fixture_archive(&archive, "Yuri", "title=Yuri\nauthor=Yuri\ncampaign=HotS\nversion=1.09\n");
    let added = add_local_mod_at(&store, &archive, &LocalModOverrides::default()).unwrap();
    assert_eq!(added.record.id, "local-yuri-of-the-swarm");
    assert!(Path::new(&added.archive_path).is_file());

    // A title override must not influence the id: progress is keyed by id.
    let renamed = sandbox.join("renamed.zip");
    write_local_fixture_archive(&renamed, "Yuri", "title=Yuri\ncampaign=HotS\n");
    let overrides = LocalModOverrides { title: Some("Totally Different Name".into()), ..Default::default() };
    let second = add_local_mod_at(&store, &renamed, &overrides).unwrap();
    assert_eq!(second.record.id, "local-renamed");
    assert_eq!(second.record.title, "Totally Different Name");

    // Remove, then add the same file again: the id must come back unchanged so
    // the player's play history stays attached.
    assert!(remove_local_mod_at(&store, "local-yuri-of-the-swarm").unwrap());
    assert!(!Path::new(&added.archive_path).exists());
    assert!(!remove_local_mod_at(&store, "local-yuri-of-the-swarm").unwrap());
    let again = add_local_mod_at(&store, &archive, &LocalModOverrides::default()).unwrap();
    assert_eq!(again.record.id, "local-yuri-of-the-swarm");

    // A non-Latin archive name still has to produce a valid campaign id.
    let chinese = sandbox.join("喵头嘤の奇妙战役.zip");
    write_local_fixture_archive(&chinese, "Eyeser", "title=Eyeser\ncampaign=LotV\n");
    let translated = add_local_mod_at(&store, &chinese, &LocalModOverrides::default()).unwrap();
    validate_campaign_id(&translated.record.id).unwrap();
    assert!(translated.record.id.starts_with("local-"));
    assert_eq!(translated.record.campaign, "Legacy of the Void");

    // Two different files with the same name must not collide.
    let nested = sandbox.join("nested");
    fs::create_dir_all(&nested).unwrap();
    let twin = nested.join("Yuri of the Swarm.zip");
    write_local_fixture_archive(&twin, "Twin", "title=Twin\ncampaign=HotS\n");
    let twin_entry = add_local_mod_at(&store, &twin, &LocalModOverrides::default()).unwrap();
    assert_eq!(twin_entry.record.id, "local-yuri-of-the-swarm-2");

    let ids = list_local_mods_at(&store).into_iter().map(|entry| entry.record.id).collect::<Vec<_>>();
    assert_eq!(ids.len(), 4);

    let _ = fs::remove_dir_all(sandbox);
}

#[test]
fn a_damaged_local_mod_list_is_survivable_for_reads_and_refused_for_writes() {
    let sandbox = std::env::temp_dir().join(format!("ccm-local-damaged-{}", Uuid::new_v4()));
    let store = sandbox.join("store");
    fs::create_dir_all(&store).unwrap();
    let archive = sandbox.join("mod.zip");
    write_local_fixture_archive(&archive, "Mod", "title=Mod\ncampaign=WoL\n");
    fs::write(store.join("local-mods.json"), b"{ not json").unwrap();

    // Listing degrades to empty instead of preventing the app from starting.
    assert!(list_local_mods_at(&store).is_empty());
    // Writing refuses, so an unparsable list is never silently overwritten.
    assert!(add_local_mod_at(&store, &archive, &LocalModOverrides::default()).is_err());
    assert!(remove_local_mod_at(&store, "local-mod").is_err());
    assert!(!local_mod_archive_directory(&store).join("local-mod.zip").exists());

    let _ = fs::remove_dir_all(sandbox);
}

#[test]
fn a_locally_added_mod_installs_from_ccms_own_copy() {
    let sandbox = std::env::temp_dir().join(format!("ccm-local-install-{}", Uuid::new_v4()));
    let store = sandbox.join("store");
    let game_dir = sandbox.join("game");
    fs::create_dir_all(game_dir.join("Maps/Campaign/swarm")).unwrap();
    fs::write(game_dir.join("Maps/Campaign/swarm/zlab01.SC2Map"), b"vanilla").unwrap();
    let archive = sandbox.join("Local HotS.zip");
    write_local_fixture_archive(&archive, "Local HotS", "title=Local HotS\nauthor=Kit\ncampaign=HotS\nversion=2.0\n");

    let entry = add_local_mod_at(&store, &archive, &LocalModOverrides::default()).unwrap();
    // The player's own file is no longer needed once CCM holds a copy.
    fs::remove_file(&archive).unwrap();

    install_campaign_blocking(InstallRequest {
        campaign_id: entry.record.id.clone(),
        title: entry.record.title.clone(),
        author: entry.record.author.clone(),
        version: entry.record.version.clone(),
        profile_dir: None,
        archive_source: entry.archive_path.clone(),
        sha256: entry.record.sha256.clone(),
        package_size: Some(entry.record.size),
        game_dir: game_dir.display().to_string(),
    })
    .unwrap();
    assert_eq!(
        fs::read(game_dir.join("Maps/Campaign/swarm/zlab01.SC2Map")).unwrap(),
        b"local-mod-map"
    );

    let _ = fs::remove_dir_all(sandbox);
}

#[test]
fn special_whole_game_root_layout_installs_the_sibling_mods_dependency() {
    // Some HotS mods are packaged the "copy Maps and Mods into StarCraft II"
    // way: top-level Maps/ and Mods/ folders with metadata.txt living inside the
    // campaign slot. The sibling Mods/ dependency tree must be installed, not
    // dropped, and loose files beside the two folders must be ignored.
    let sandbox = std::env::temp_dir().join(format!("ccm-install-root-{}", Uuid::new_v4()));
    let game_dir = sandbox.join("game");
    let archive_path = sandbox.join("leviathan-like.zip");
    fs::create_dir_all(game_dir.join("Maps/Campaign/swarm/evolution")).unwrap();
    fs::create_dir_all(game_dir.join("Mods")).unwrap();

    let mut archive = ZipWriter::new(File::create(&archive_path).unwrap());
    let options = SimpleFileOptions::default();
    archive.start_file("Maps/Campaign/swarm/metadata.txt", options).unwrap();
    archive.write_all(b"title=Install Root\nauthor=Test\ncampaign=HotS\nversion=1\n").unwrap();
    for (path, bytes) in [
        ("Maps/Campaign/swarm/zchar01.SC2Map", b"campaign-map" as &[u8]),
        ("Maps/Campaign/swarm/Evolution/zevolutionzergling.SC2Map", b"evolution-map"),
        ("Mods/MyDep.SC2Mod/DocumentInfo", b"dependency-file"),
        ("Change Logs.txt", b"loose changelog beside the two folders"),
    ] {
        archive.start_file(path, options).unwrap();
        archive.write_all(bytes).unwrap();
    }
    archive.finish().unwrap();

    let request = InstallRequest {
        campaign_id: "install-root-test".into(), title: "Install Root".into(), author: "Test".into(), version: "1".into(),
        profile_dir: None, archive_source: archive_path.display().to_string(), sha256: sha256_file(&archive_path).unwrap(),
        package_size: None, game_dir: game_dir.display().to_string(),
    };
    let plan = plan_campaign_install_blocking(request.clone()).unwrap();
    assert!(plan.dependency_roots.iter().any(|root| root == "Mods/MyDep.SC2Mod"), "dependency root must be planned");

    let result = install_archive(&game_dir, &request, &archive_path, &request.sha256, None).unwrap();
    // metadata.txt + two maps + one dependency file; the loose changelog is skipped.
    assert_eq!(result.files_installed, 4);
    assert_eq!(fs::read(game_dir.join("Maps/Campaign/swarm/zchar01.SC2Map")).unwrap(), b"campaign-map");
    assert_eq!(fs::read(game_dir.join("Maps/Campaign/swarm/Evolution/zevolutionzergling.SC2Map")).unwrap(), b"evolution-map");
    assert_eq!(fs::read(game_dir.join("Mods/MyDep.SC2Mod/DocumentInfo")).unwrap(), b"dependency-file");
    assert!(!game_dir.join("Change Logs.txt").exists(), "loose root files must not be installed");

    let _ = fs::remove_dir_all(sandbox);
}

#[test]
fn plan_warns_when_most_archive_files_are_outside_the_installed_layout() {
    // A guard against silently installing only part of an archive: if the plan
    // matches far fewer files than the archive holds, it must say so.
    let sandbox = std::env::temp_dir().join(format!("ccm-coverage-warn-{}", Uuid::new_v4()));
    let game_dir = sandbox.join("game");
    let archive_path = sandbox.join("mostly-unmatched.zip");
    fs::create_dir_all(game_dir.join("Maps/Campaign/swarm")).unwrap();
    fs::create_dir_all(game_dir.join("Mods")).unwrap();

    let mut archive = ZipWriter::new(File::create(&archive_path).unwrap());
    let options = SimpleFileOptions::default();
    archive.start_file("Mod/metadata.txt", options).unwrap();
    archive.write_all(b"title=Mostly Unmatched\nauthor=Test\ncampaign=HotS\nversion=1\n").unwrap();
    archive.start_file("Mod/zchar01.SC2Map", options).unwrap();
    archive.write_all(b"the-one-map").unwrap();
    // 60 files that sit outside the content root, so the plan cannot place them.
    for i in 0..60 {
        archive.start_file(format!("Extras/file_{i:03}.bin"), options).unwrap();
        archive.write_all(b"x").unwrap();
    }
    archive.finish().unwrap();

    let request = InstallRequest {
        campaign_id: "coverage-warn".into(), title: "Mostly Unmatched".into(), author: "Test".into(), version: "1".into(),
        profile_dir: None, archive_source: archive_path.display().to_string(), sha256: sha256_file(&archive_path).unwrap(),
        package_size: None, game_dir: game_dir.display().to_string(),
    };
    let plan = plan_campaign_install_blocking(request).unwrap();
    assert!(plan.warnings.iter().any(|w| w.contains("were skipped")), "expected a coverage warning, got {:?}", plan.warnings);

    let _ = fs::remove_dir_all(sandbox);
}

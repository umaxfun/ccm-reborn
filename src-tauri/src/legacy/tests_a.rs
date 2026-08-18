    #[test]
    fn wol_target_boundary_preserves_other_campaign_branches() {
        let sandbox = std::env::temp_dir().join(format!("ccm-wol-boundary-{}", Uuid::new_v4()));
        let wol = sandbox.join("Maps/Campaign");
        let swarm = wol.join("swarm");
        let void = wol.join("void");
        fs::create_dir_all(&swarm).unwrap();
        fs::create_dir_all(&void).unwrap();
        fs::write(wol.join("liberty.SC2Map"), b"wol").unwrap();
        fs::write(swarm.join("zchar01.SC2Map"), b"hots").unwrap();
        fs::write(void.join("pchar01.SC2Map"), b"lotv").unwrap();

        let files = collect_campaign_target_files(&sandbox, "Maps/Campaign").unwrap();
        assert_eq!(files, vec![wol.join("liberty.SC2Map")]);
        clear_campaign_target(&sandbox, "Maps/Campaign").unwrap();
        assert!(!wol.join("liberty.SC2Map").exists());
        assert_eq!(fs::read(swarm.join("zchar01.SC2Map")).unwrap(), b"hots");
        assert_eq!(fs::read(void.join("pchar01.SC2Map")).unwrap(), b"lotv");
        let _ = fs::remove_dir_all(sandbox);
    }

    #[test]
    fn campaign_slots_install_update_and_restore_independently() {
        let sandbox = std::env::temp_dir().join(format!("ccm-slot-states-{}", Uuid::new_v4()));
        let game_dir = sandbox.join("game");
        fs::create_dir_all(game_dir.join("Maps/Campaign/swarm")).unwrap();
        fs::write(game_dir.join("Maps/Campaign/wol-original.SC2Map"), b"wol-original").unwrap();
        fs::write(game_dir.join("Maps/Campaign/swarm/hots-original.SC2Map"), b"hots-original").unwrap();
        let make_archive = |path: &Path, folder: &str, campaign: &str, file: &str, bytes: &[u8]| {
            let file_handle = File::create(path).unwrap();
            let mut archive = ZipWriter::new(file_handle);
            let options = SimpleFileOptions::default();
            archive.start_file(format!("{folder}/metadata.txt"), options).unwrap();
            archive.write_all(format!("title={folder}\nauthor=Test\ncampaign={campaign}\nversion=1\n").as_bytes()).unwrap();
            archive.start_file(format!("{folder}/{file}"), options).unwrap();
            archive.write_all(bytes).unwrap();
            archive.finish().unwrap();
        };
        let hots_archive = sandbox.join("hots.zip");
        let wol_archive = sandbox.join("wol.zip");
        make_archive(&hots_archive, "HotS", "HotS", "zchar01.SC2Map", b"hots-custom");
        make_archive(&wol_archive, "WoL", "WoL", "liberty01.SC2Map", b"wol-custom");
        let request = |archive: &Path, id: &str, title: &str| InstallRequest {
            campaign_id: id.into(), title: title.into(), author: "Test".into(), version: "1".into(),
            profile_dir: None, archive_source: archive.display().to_string(), sha256: sha256_file(archive).unwrap(), package_size: None,
            game_dir: game_dir.display().to_string(),
        };
        install_campaign_blocking(request(&hots_archive, "test-hots", "HotS")).unwrap();
        // Existing users have one legacy global state.json. It must remain
        // owned by HotS while WoL begins using its own slot state.
        let legacy_hots = read_state_for_target(&game_dir, "Maps/Campaign/swarm").unwrap().unwrap();
        write_json_atomic(&state_path(&game_dir), &legacy_hots).unwrap();
        fs::remove_file(state_path_for_target(&game_dir, "Maps/Campaign/swarm").unwrap()).unwrap();
        let wol_request = request(&wol_archive, "test-wol", "WoL");
        let plan = plan_campaign_install_blocking(wol_request.clone()).unwrap();
        assert_eq!(plan.update_kind, "fresh-install");
        assert!(plan.previous_install_campaign_id.is_none());
        assert!(plan.file_changes.iter().all(|change| !change.destination.contains("Maps/Campaign/swarm")));
        install_campaign_blocking(wol_request).unwrap();
        assert_eq!(fs::read(game_dir.join("Maps/Campaign/swarm/zchar01.SC2Map")).unwrap(), b"hots-custom");
        assert_eq!(fs::read(game_dir.join("Maps/Campaign/liberty01.SC2Map")).unwrap(), b"wol-custom");
        assert_eq!(read_managed_states(&game_dir).unwrap().len(), 2);
        restore_existing_campaign(&game_dir, "Maps/Campaign").unwrap();
        assert_eq!(fs::read(game_dir.join("Maps/Campaign/swarm/zchar01.SC2Map")).unwrap(), b"hots-custom");
        assert!(read_state_for_target(&game_dir, "Maps/Campaign/swarm").unwrap().is_some());
        assert!(read_state_for_target(&game_dir, "Maps/Campaign").unwrap().is_none());
        let _ = fs::remove_dir_all(sandbox);
    }

    #[test]
    fn repair_replaces_a_shared_mod_dependency_without_erasing_another_slot() {
        let sandbox = std::env::temp_dir().join(format!("ccm-shared-mod-repair-{}", Uuid::new_v4()));
        let game_dir = sandbox.join("game");
        fs::create_dir_all(game_dir.join("Maps/Campaign/swarm")).unwrap();
        fs::create_dir_all(game_dir.join("Maps/Campaign")).unwrap();
        fs::create_dir_all(game_dir.join("Mods/Shared.SC2Mod")).unwrap();
        fs::write(game_dir.join("Mods/Shared.SC2Mod/Core.txt"), b"vanilla-shared").unwrap();
        let make_archive = |path: &Path, folder: &str, campaign: &str, mission: &str, dependency: &[u8], extra: bool| {
            let file_handle = File::create(path).unwrap();
            let mut archive = ZipWriter::new(file_handle);
            let options = SimpleFileOptions::default();
            archive.start_file(format!("{folder}/metadata.txt"), options).unwrap();
            archive.write_all(format!("title={folder}\nauthor=Test\ncampaign={campaign}\nversion=1\n").as_bytes()).unwrap();
            archive.start_file(format!("{folder}/mission.SC2Map"), options).unwrap();
            archive.write_all(mission.as_bytes()).unwrap();
            archive.start_file(format!("{folder}/Shared.SC2Mod/Core.txt"), options).unwrap();
            archive.write_all(dependency).unwrap();
            if extra {
                archive.start_file(format!("{folder}/Shared.SC2Mod/OtherSlot.txt"), options).unwrap();
                archive.write_all(b"wol-only").unwrap();
            }
            archive.finish().unwrap();
        };
        let hots_archive = sandbox.join("hots.zip");
        let wol_archive = sandbox.join("wol.zip");
        make_archive(&hots_archive, "HotS", "HotS", "hots-custom", b"hots-shared-v1", false);
        make_archive(&wol_archive, "WoL", "WoL", "wol-custom", b"wol-shared-v2", true);
        let request = |archive: &Path, id: &str, title: &str| InstallRequest {
            campaign_id: id.into(), title: title.into(), author: "Test".into(), version: "1".into(),
            profile_dir: None, archive_source: archive.display().to_string(), sha256: sha256_file(archive).unwrap(), package_size: None,
            game_dir: game_dir.display().to_string(),
        };
        let hots_request = request(&hots_archive, "test-hots", "HotS");
        install_campaign_blocking(hots_request.clone()).unwrap();
        install_campaign_blocking(request(&wol_archive, "test-wol", "WoL")).unwrap();
        assert_eq!(fs::read(game_dir.join("Mods/Shared.SC2Mod/Core.txt")).unwrap(), b"wol-shared-v2");
        assert_eq!(fs::read(game_dir.join("Mods/Shared.SC2Mod/OtherSlot.txt")).unwrap(), b"wol-only");
        restore_existing_campaign(&game_dir, "Maps/Campaign/swarm").unwrap();
        assert_eq!(fs::read(game_dir.join("Mods/Shared.SC2Mod/Core.txt")).unwrap(), b"wol-shared-v2");
        assert_eq!(fs::read(game_dir.join("Mods/Shared.SC2Mod/OtherSlot.txt")).unwrap(), b"wol-only");
        install_campaign_blocking(hots_request).unwrap();
        assert_eq!(fs::read(game_dir.join("Mods/Shared.SC2Mod/Core.txt")).unwrap(), b"hots-shared-v1");
        assert_eq!(fs::read(game_dir.join("Maps/Campaign/mission.SC2Map")).unwrap(), b"wol-custom");
        assert!(read_state_for_target(&game_dir, "Maps/Campaign").unwrap().is_some());
        assert!(read_state_for_target(&game_dir, "Maps/Campaign/swarm").unwrap().is_some());
        restore_existing_campaign(&game_dir, "Maps/Campaign").unwrap();
        assert_eq!(fs::read(game_dir.join("Mods/Shared.SC2Mod/Core.txt")).unwrap(), b"hots-shared-v1");
        restore_existing_campaign(&game_dir, "Maps/Campaign/swarm").unwrap();
        assert_eq!(fs::read(game_dir.join("Mods/Shared.SC2Mod/Core.txt")).unwrap(), b"vanilla-shared");
        assert!(!game_dir.join("Mods/Shared.SC2Mod/OtherSlot.txt").exists());
        let _ = fs::remove_dir_all(sandbox);
    }

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
            author: "Test".into(),
            version: "1.0".into(),
            profile_dir: None,
            archive_source: archive_path.display().to_string(),
            sha256: sha256_file(&archive_path).unwrap(),
            package_size: None,
            game_dir: game_dir.display().to_string(),
        };
        let result = install_archive(&game_dir, &request, &archive_path, &request.sha256, None).unwrap();
        assert_eq!(result.files_installed, 3);
        assert_eq!(fs::read(campaign_dir.join("zchar01.SC2Map")).unwrap(), b"custom mission");
        assert!(!campaign_dir.join("obsolete.SC2Map").exists());
        assert!(!campaign_dir.join("TestDependency.SC2Mod").exists());
        assert_eq!(fs::read(game_dir.join("Mods/TestDependency.SC2Mod")).unwrap(), b"custom dependency");

        let restored = restore_original_campaigns_blocking(game_dir.display().to_string(), None, "Maps/Campaign/swarm".into()).unwrap();
        assert_eq!(restored.conflicts.len(), 0);
        assert_eq!(fs::read(campaign_dir.join("zchar01.SC2Map")).unwrap(), b"vanilla mission");
        assert_eq!(fs::read(campaign_dir.join("obsolete.SC2Map")).unwrap(), b"vanilla obsolete");
        assert_eq!(fs::read(game_dir.join("Mods/TestDependency.SC2Mod")).unwrap(), b"old dependency");
        let _ = fs::remove_dir_all(sandbox);
    }

    #[test]
    fn live_install_requires_an_explicit_account_profile() {
        let root = std::env::temp_dir().join(format!("ccm-live-profile-required-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("StarCraft II.app")).unwrap();
        let error = install_campaign_blocking(InstallRequest {
            campaign_id: "test".into(),
            title: "Test".into(),
            author: "Test".into(),
            version: "1".into(),
            profile_dir: None,
            archive_source: root.join("missing.zip").display().to_string(),
            sha256: "a".repeat(64),
            package_size: None,
            game_dir: root.display().to_string(),
        })
        .unwrap_err();
        assert!(error.contains("--profile-dir"));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn install_rejects_a_symlinked_game_ancestor_before_mutation() {
        use std::os::unix::fs::symlink;

        let sandbox = std::env::temp_dir().join(format!("ccm-symlink-game-{}", Uuid::new_v4()));
        let game = sandbox.join("game");
        let outside = sandbox.join("outside");
        let archive = sandbox.join("package.zip");
        fs::create_dir_all(&game).unwrap();
        fs::create_dir_all(&outside).unwrap();
        symlink(&outside, game.join("Maps")).unwrap();
        let file = File::create(&archive).unwrap();
        let mut zip = ZipWriter::new(file);
        zip.start_file("Symlink Test/metadata.txt", SimpleFileOptions::default()).unwrap();
        zip.write_all(b"title=Symlink Test\nauthor=Test\ncampaign=WoL\nversion=1\n").unwrap();
        zip.start_file("Symlink Test/mission.SC2Map", SimpleFileOptions::default()).unwrap();
        zip.write_all(b"must not escape").unwrap();
        zip.finish().unwrap();
        let error = install_campaign_blocking(InstallRequest {
            campaign_id: "symlink-test".into(), title: "Symlink Test".into(), author: "Test".into(),
            version: "1".into(), profile_dir: None, archive_source: archive.display().to_string(),
            sha256: sha256_file(&archive).unwrap(), package_size: None, game_dir: game.display().to_string(),
        }).unwrap_err();
        assert!(error.contains("symlinked game path"), "{error}");
        assert!(!outside.join("Campaign/mission.SC2Map").exists());
        let _ = fs::remove_dir_all(sandbox);
    }

    #[test]
    fn operation_lock_rejects_a_second_writer_for_the_same_game() {
        let root = std::env::temp_dir().join(format!("ccm-lock-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let first = acquire_operation_lock(&root).unwrap();
        let error = acquire_operation_lock(&root).unwrap_err();
        assert!(error.contains("Another CCM operation"), "{error}");
        drop(first);
        assert!(acquire_operation_lock(&root).is_ok());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn malformed_state_cannot_claim_a_second_campaign_target() {
        let state = ManagedState {
            format: 1, campaign_id: "malformed".into(), title: "Malformed".into(), author: String::new(),
            version: String::new(), target_path: "Maps/Campaign".into(), installed_at: 0,
            backup_dir: "backups/test".into(),
            cleared_directories: vec!["Maps/Campaign".into(), "Maps/Campaign/swarm".into()],
            files: Vec::new(),
        };
        assert!(validate_managed_state(&state).is_err());
    }

    #[test]
    fn malformed_state_cannot_escape_the_backup_directory() {
        let state = ManagedState {
            format: 1, campaign_id: "malformed".into(), title: "Malformed".into(), author: String::new(),
            version: String::new(), target_path: "Maps/Campaign".into(), installed_at: 0,
            backup_dir: "../../outside".into(), cleared_directories: vec!["Maps/Campaign".into()], files: Vec::new(),
        };
        assert!(validate_managed_state(&state).is_err());
    }

    #[test]
    fn update_removes_only_files_recorded_by_previous_install_before_copying_new_package() {
        let sandbox = std::env::temp_dir().join(format!("ccm-reborn-update-{}", Uuid::new_v4()));
        fs::create_dir_all(&sandbox).unwrap();
        let game_dir = sandbox.join("game");
        let campaign_dir = game_dir.join("Maps/Campaign");
        fs::create_dir_all(&campaign_dir).unwrap();
        fs::write(campaign_dir.join("vanilla.SC2Map"), b"vanilla").unwrap();

        let make_archive = |path: &Path, version: &str, include_old: bool| {
            let file = File::create(path).unwrap();
            let mut archive = ZipWriter::new(file);
            let options = SimpleFileOptions::default();
            archive.start_file("Nightmare/metadata.txt", options).unwrap();
            archive
                .write_all(format!("title=Nightmare\nauthor=Test\ncampaign=WoL\nversion={version}\n").as_bytes())
                .unwrap();
            archive.start_file("Nightmare/new.SC2Map", options).unwrap();
            archive.write_all(format!("new-{version}").as_bytes()).unwrap();
            if include_old {
                archive.start_file("Nightmare/old.SC2Map", options).unwrap();
                archive.write_all(b"old-v1").unwrap();
            }
            archive.finish().unwrap();
        };

        let v1 = sandbox.join("v1.zip");
        let v2 = sandbox.join("v2.zip");
        make_archive(&v1, "1.34", true);
        make_archive(&v2, "1.35", false);
        let request = |archive: &Path, version: &str| InstallRequest {
            campaign_id: "nightmare-v1".into(),
            title: "Nightmare".into(),
            author: "Test".into(),
            version: version.into(),
            profile_dir: None,
            archive_source: archive.display().to_string(),
            sha256: sha256_file(archive).unwrap(),
            package_size: None,
            game_dir: game_dir.display().to_string(),
        };

        install_campaign_blocking(request(&v1, "1.34")).unwrap();
        assert!(game_dir.join("Maps/Campaign/old.SC2Map").is_file());
        let manifest_path = installed_manifest_path(&game_dir, "Maps/Campaign").unwrap();
        let first_manifest = profile_core::read_installed_manifest(&manifest_path).unwrap();
        assert_eq!(first_manifest.version, "1.34");
        assert_eq!(first_manifest.files.len(), 3);

        let update_plan = plan_campaign_install_blocking(request(&v2, "1.35")).unwrap();
        assert_eq!(update_plan.update_kind, "update-existing-install");
        assert_eq!(update_plan.previous_install_files, 3);
        assert_eq!(update_plan.previous_install_version.as_deref(), Some("1.34"));
        assert!(update_plan.file_changes.iter().any(|change| {
            change.operation == "remove previous package file before update"
                && change.destination.ends_with("old.SC2Map")
        }));

        install_campaign_blocking(request(&v2, "1.35")).unwrap();
        assert!(!game_dir.join("Maps/Campaign/old.SC2Map").exists());
        assert_eq!(fs::read(game_dir.join("Maps/Campaign/new.SC2Map")).unwrap(), b"new-1.35");
        let second_manifest = profile_core::read_installed_manifest(&manifest_path).unwrap();
        assert_eq!(second_manifest.version, "1.35");
        assert_eq!(second_manifest.files.len(), 2);
        assert!(!second_manifest.files.iter().any(|file| file.destination.ends_with("old.SC2Map")));
        let history = collect_regular_files(&game_dir.join(".ccm-reborn/installed/history")).unwrap();
        assert!(history.iter().any(|path| {
            profile_core::read_installed_manifest(path)
                .map(|manifest| manifest.version == "1.34")
                .unwrap_or(false)
        }));

        let _ = fs::remove_dir_all(sandbox);
    }

    #[test]
    fn interrupted_update_recovers_the_previous_managed_campaign_not_only_vanilla() {
        let sandbox = std::env::temp_dir().join(format!("ccm-update-rollback-{}", Uuid::new_v4()));
        let game_dir = sandbox.join("game");
        let campaign_dir = game_dir.join("Maps/Campaign");
        fs::create_dir_all(&campaign_dir).unwrap();
        fs::write(campaign_dir.join("vanilla.SC2Map"), b"vanilla").unwrap();
        let make_archive = |path: &Path, version: &str, files: &[(&str, &[u8])]| {
            let file = File::create(path).unwrap();
            let mut archive = ZipWriter::new(file);
            let options = SimpleFileOptions::default();
            archive.start_file("Rollback/metadata.txt", options).unwrap();
            archive
                .write_all(format!("title=Rollback\nauthor=Test\ncampaign=WoL\nversion={version}\n").as_bytes())
                .unwrap();
            for (name, bytes) in files {
                archive.start_file(format!("Rollback/{name}"), options).unwrap();
                archive.write_all(bytes).unwrap();
            }
            archive.finish().unwrap();
        };
        let v1 = sandbox.join("v1.zip");
        let v2 = sandbox.join("v2.zip");
        make_archive(&v1, "1.0", &[("mission.SC2Map", b"v1"), ("old.SC2Map", b"old")]);
        make_archive(&v2, "1.1", &[("mission.SC2Map", b"v2")]);
        let request = |archive: &Path, version: &str| InstallRequest {
            campaign_id: "rollback-v1".into(),
            title: "Rollback".into(),
            author: "Test".into(),
            version: version.into(),
            profile_dir: None,
            archive_source: archive.display().to_string(),
            sha256: sha256_file(archive).unwrap(),
            package_size: None,
            game_dir: game_dir.display().to_string(),
        };
        install_campaign_blocking(request(&v1, "1.0")).unwrap();

        let staging = game_dir.join(".ccm-reborn/staging/rollback-test");
        fs::create_dir_all(&staging).unwrap();
        let package = read_ccm_package(&v2).unwrap();
        let previous = snapshot_previous_install(&game_dir, &staging, &package.target_path).unwrap().unwrap();
        assert_eq!(restore_existing_campaign(&game_dir, &package.target_path).unwrap().conflicts, Vec::<String>::new());
        let staged = extract_ccm_package(&v2, &package, &staging.join("new-package")).unwrap();
        let new_request = request(&v2, "1.1");
        let new_state = backup_campaign_directory(&game_dir, &new_request, &package.target_path, &staged).unwrap();
        clear_directory(&game_dir.join(&package.target_path)).unwrap();
        for file in &staged {
            copy_file(&file.path, &game_dir.join(&file.destination)).unwrap();
        }
        let manifest_path = installed_manifest_path(&game_dir, &package.target_path).unwrap();
        write_pending_install_journal(
            &game_dir,
            &PendingInstallJournal {
                format: 1,
                profile_transaction: ProfileTransaction { entries: Vec::new() },
                profile_roots: Vec::new(),
                previous_install: Some(previous),
                new_state: Some(new_state),
                completed: false,
            },
        )
        .unwrap();
        assert!(recover_interrupted_install(&game_dir).unwrap());
        assert_eq!(fs::read(campaign_dir.join("mission.SC2Map")).unwrap(), b"v1");
        assert_eq!(fs::read(campaign_dir.join("old.SC2Map")).unwrap(), b"old");
        assert!(!campaign_dir.join("vanilla.SC2Map").exists());
        assert_eq!(read_state_for_target(&game_dir, &package.target_path).unwrap().unwrap().version, "1.0");
        assert_eq!(profile_core::read_installed_manifest(&manifest_path).unwrap().version, "1.0");
        assert!(!pending_install_path(&game_dir).exists());
        let _ = fs::remove_dir_all(sandbox);
    }

    #[test]
    fn recovery_rebuilds_previous_mod_when_old_state_survives_a_partial_restore() {
        let sandbox = std::env::temp_dir().join(format!("ccm-stale-state-recovery-{}", Uuid::new_v4()));
        let game_dir = sandbox.join("game");
        let campaign_dir = game_dir.join("Maps/Campaign");
        fs::create_dir_all(&campaign_dir).unwrap();
        fs::write(campaign_dir.join("mission.SC2Map"), b"vanilla").unwrap();
        let archive_path = sandbox.join("v1.zip");
        let archive_file = File::create(&archive_path).unwrap();
        let mut archive = ZipWriter::new(archive_file);
        let options = SimpleFileOptions::default();
        archive.start_file("Recovery/metadata.txt", options).unwrap();
        archive
            .write_all(b"title=Recovery\nauthor=Test\ncampaign=WoL\nversion=1.0\n")
            .unwrap();
        archive.start_file("Recovery/mission.SC2Map", options).unwrap();
        archive.write_all(b"yuri-like-custom").unwrap();
        archive.finish().unwrap();
        let request = InstallRequest {
            campaign_id: "recovery-v1".into(),
            title: "Recovery".into(),
            author: "Test".into(),
            version: "1.0".into(),
            profile_dir: None,
            archive_source: archive_path.display().to_string(),
            sha256: sha256_file(&archive_path).unwrap(),
            package_size: None,
            game_dir: game_dir.display().to_string(),
        };
        install_campaign_blocking(request).unwrap();
        let staging = game_dir.join(".ccm-reborn/staging/stale-state");
        fs::create_dir_all(&staging).unwrap();
        let previous = snapshot_previous_install(&game_dir, &staging, "Maps/Campaign").unwrap().unwrap();

        // Simulates a process dying after `force_restore(old_state)` but
        // before `state.json` and the old backup are retired.
        force_restore(&game_dir, &previous.state).unwrap();
        assert_eq!(fs::read(campaign_dir.join("mission.SC2Map")).unwrap(), b"vanilla");
        assert_eq!(read_state_for_target(&game_dir, "Maps/Campaign").unwrap().unwrap().campaign_id, "recovery-v1");
        write_pending_install_journal(
            &game_dir,
            &PendingInstallJournal {
                format: 1,
                profile_transaction: ProfileTransaction { entries: Vec::new() },
                profile_roots: Vec::new(),
                previous_install: Some(previous),
                new_state: None,
                completed: false,
            },
        )
        .unwrap();

        assert!(recover_interrupted_install(&game_dir).unwrap());
        assert_eq!(fs::read(campaign_dir.join("mission.SC2Map")).unwrap(), b"yuri-like-custom");
        assert_eq!(read_state_for_target(&game_dir, "Maps/Campaign").unwrap().unwrap().campaign_id, "recovery-v1");
        assert!(!pending_install_path(&game_dir).exists());
        let _ = fs::remove_dir_all(sandbox);
    }


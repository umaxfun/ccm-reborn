    #[test]
    fn update_refuses_a_drifted_previous_package_file_without_touching_it() {
        let sandbox = std::env::temp_dir().join(format!("ccm-reborn-update-drift-{}", Uuid::new_v4()));
        fs::create_dir_all(&sandbox).unwrap();
        let game_dir = sandbox.join("game");
        fs::create_dir_all(&game_dir).unwrap();
        let archive_path = sandbox.join("mod.zip");
        let file = File::create(&archive_path).unwrap();
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        archive.start_file("Nightmare/metadata.txt", options).unwrap();
        archive.write_all(b"title=Nightmare\nauthor=Test\ncampaign=WoL\nversion=1.0\n").unwrap();
        archive.start_file("Nightmare/mission.SC2Map", options).unwrap();
        archive.write_all(b"managed").unwrap();
        archive.finish().unwrap();

        let request = InstallRequest {
            campaign_id: "nightmare-v1".into(),
            title: "Nightmare".into(),
            author: "Test".into(),
            version: "1.0".into(),
            profile_dir: None,
            archive_source: archive_path.display().to_string(),
            sha256: sha256_file(&archive_path).unwrap(),
            package_size: None,
            game_dir: game_dir.display().to_string(),
        };
        install_campaign_blocking(request).unwrap();
        fs::write(game_dir.join("Maps/Campaign/mission.SC2Map"), b"user changed").unwrap();

        let error = install_campaign_blocking(InstallRequest {
            campaign_id: "nightmare-v1".into(),
            title: "Nightmare".into(),
            author: "Test".into(),
            version: "1.1".into(),
            profile_dir: None,
            archive_source: archive_path.display().to_string(),
            sha256: sha256_file(&archive_path).unwrap(),
            package_size: None,
            game_dir: game_dir.display().to_string(),
        })
        .unwrap_err();
        assert!(error.contains("changed outside CCM Reborn"), "{error}");
        assert_eq!(fs::read(game_dir.join("Maps/Campaign/mission.SC2Map")).unwrap(), b"user changed");
        let _ = fs::remove_dir_all(sandbox);
    }

    #[test]
    fn profile_transition_round_trips_campaign_bank_and_only_target_progress_node() {
        let sandbox = std::env::temp_dir().join(format!("ccm-profile-roundtrip-{}", Uuid::new_v4()));
        let profile = sandbox.join("sc2-profile");
        let profile_base = sandbox.join("profiles");
        let staging = sandbox.join("staging");
        fs::create_dir_all(profile.join("Banks")).unwrap();
        fs::create_dir_all(profile.join("Saves")).unwrap();
        fs::create_dir_all(&staging).unwrap();
        fs::write(profile.join("Banks/ZCampaign.SC2Bank"), b"yuri-bank").unwrap();
        fs::write(
            profile.join("CampaignProgress.xml"),
            "<root>\n  <CampaignProgress id=\"Swarm\" tutorialfinished=\"1\" campaignfinished=\"1\" />\n  <CampaignProgress id=\"Liberty\" tutorialfinished=\"7\" campaignfinished=\"8\" />\n</root>\n",
        )
        .unwrap();
        fs::create_dir_all(profile_base.join("abathur/Banks")).unwrap();
        fs::write(profile_base.join("abathur/Banks/ZCampaign.SC2Bank"), b"abathur-bank").unwrap();
        fs::write(
            profile_base.join("abathur/CampaignProgress.xml"),
            "<root>\n  <CampaignProgress id=\"Swarm\" tutorialfinished=\"0\" campaignfinished=\"0\" />\n  <CampaignProgress id=\"Liberty\" tutorialfinished=\"99\" campaignfinished=\"99\" />\n</root>\n",
        )
        .unwrap();

        let yuri_deps = vec![PathBuf::from("Mods/YuriCampaign.SC2Mod")];
        let abathur_deps = vec![PathBuf::from("Mods/Abathur.SC2Mod")];
        let first = apply_profile_transition_at(
            &sandbox,
            &profile,
            &profile_base,
            "Maps/Campaign/swarm",
            "Maps/Campaign/swarm",
            "yuri",
            &yuri_deps,
            "abathur",
            &abathur_deps,
            &staging,
        )
        .unwrap();
        assert_eq!(fs::read(profile.join("Banks/ZCampaign.SC2Bank")).unwrap(), b"abathur-bank");
        let switched = fs::read_to_string(profile.join("CampaignProgress.xml")).unwrap();
        assert!(switched.contains("Swarm\" tutorialfinished=\"0\" campaignfinished=\"0\""));
        assert!(switched.contains("Liberty\" tutorialfinished=\"7\" campaignfinished=\"8\""));
        assert_eq!(fs::read(profile_base.join("yuri/Banks/ZCampaign.SC2Bank")).unwrap(), b"yuri-bank");

        let second = apply_profile_transition_at(
            &sandbox,
            &profile,
            &profile_base,
            "Maps/Campaign/swarm",
            "Maps/Campaign/swarm",
            "abathur",
            &abathur_deps,
            "yuri",
            &yuri_deps,
            &staging,
        )
        .unwrap();
        assert_eq!(fs::read(profile.join("Banks/ZCampaign.SC2Bank")).unwrap(), b"yuri-bank");
        let restored = fs::read_to_string(profile.join("CampaignProgress.xml")).unwrap();
        assert!(restored.contains("Swarm\" tutorialfinished=\"1\" campaignfinished=\"1\""));
        assert!(restored.contains("Liberty\" tutorialfinished=\"7\" campaignfinished=\"8\""));
        assert_eq!(fs::read(profile_base.join("abathur/Banks/ZCampaign.SC2Bank")).unwrap(), b"abathur-bank");
        let _ = first;
        let _ = second;
        let _ = fs::remove_dir_all(sandbox);
    }

    #[test]
    fn profile_transition_rollback_restores_live_profile_and_removes_new_snapshot_files() {
        let sandbox = std::env::temp_dir().join(format!("ccm-profile-rollback-{}", Uuid::new_v4()));
        let profile = sandbox.join("sc2-profile");
        let profile_base = sandbox.join("profiles");
        let staging = sandbox.join("staging");
        fs::create_dir_all(profile.join("Banks")).unwrap();
        fs::create_dir_all(&staging).unwrap();
        fs::write(profile.join("Banks/ZCampaign.SC2Bank"), b"yuri-bank").unwrap();
        fs::write(
            profile.join("CampaignProgress.xml"),
            "<CampaignProgress id=\"Swarm\" tutorialfinished=\"1\" campaignfinished=\"1\" />\n",
        )
        .unwrap();
        fs::create_dir_all(profile_base.join("abathur/Banks")).unwrap();
        fs::write(profile_base.join("abathur/Banks/ZCampaign.SC2Bank"), b"abathur-bank").unwrap();
        fs::write(
            profile_base.join("abathur/CampaignProgress.xml"),
            "<CampaignProgress id=\"Swarm\" tutorialfinished=\"0\" campaignfinished=\"0\" />\n",
        )
        .unwrap();
        let transaction = apply_profile_transition_at(
            &sandbox,
            &profile,
            &profile_base,
            "Maps/Campaign/swarm",
            "Maps/Campaign/swarm",
            "yuri",
            &[PathBuf::from("Mods/YuriCampaign.SC2Mod")],
            "abathur",
            &[PathBuf::from("Mods/Abathur.SC2Mod")],
            &staging,
        )
        .unwrap();
        assert_eq!(fs::read(profile.join("Banks/ZCampaign.SC2Bank")).unwrap(), b"abathur-bank");
        transaction.rollback().unwrap();
        assert_eq!(fs::read(profile.join("Banks/ZCampaign.SC2Bank")).unwrap(), b"yuri-bank");
        assert!(!profile_base.join("yuri/Banks/ZCampaign.SC2Bank").exists());
        assert!(fs::read_to_string(profile.join("CampaignProgress.xml")).unwrap().contains("tutorialfinished=\"1\""));
        let _ = fs::remove_dir_all(sandbox);
    }

    #[test]
    fn every_slot_moves_opaque_campaign_saves_and_companion_banks() {
        for (target, marker) in [
            ("Maps/Campaign", "Liberty"),
            ("Maps/Campaign/swarm", "Swarm"),
            ("Maps/Campaign/void", "Void"),
            ("Maps/Campaign/nova", "Nova"),
        ] {
            let sandbox = std::env::temp_dir().join(format!("ccm-slot-payload-{}-{}", marker, Uuid::new_v4()));
            let profile = sandbox.join("profile");
            let profile_base = sandbox.join("profiles");
            let staging = sandbox.join("staging");
            let spec = campaign_profile_spec(target).unwrap();
            fs::create_dir_all(profile.join("Banks")).unwrap();
            fs::create_dir_all(profile.join("Saves")).unwrap();
            fs::create_dir_all(&staging).unwrap();
            for bank in spec.banks {
                fs::write(profile.join("Banks").join(bank), format!("{marker}-{bank}")).unwrap();
            }
            let opaque_save = profile.join("Saves").join(format!("{marker}CampaignCompletedSave.SC2Save"));
            fs::write(&opaque_save, b"opaque save without save.details").unwrap();
            fs::write(
                profile.join("CampaignProgress.xml"),
                format!("<CampaignProgress id=\"{}\" tutorialfinished=\"1\" campaignfinished=\"1\" />\n", spec.progress_ids[0]),
            )
            .unwrap();

            apply_profile_transition_at(
                &sandbox,
                &profile,
                &profile_base,
                target,
                target,
                "source-mod",
                &[],
                "target-mod",
                &[],
                &staging,
            )
            .unwrap();

            assert!(!opaque_save.exists(), "{target} opaque save stayed live");
            assert!(profile_base.join("source-mod").join("Saves").join(format!("{marker}CampaignCompletedSave.SC2Save")).is_file());
            for bank in spec.banks {
                assert!(!profile.join("Banks").join(bank).exists(), "{target} bank {bank} stayed live");
                assert!(profile_base.join("source-mod/Banks").join(bank).is_file());
            }
            let progress = fs::read_to_string(profile.join("CampaignProgress.xml")).unwrap();
            assert!(progress.contains("tutorialfinished=\"0\""));
            assert!(progress.contains("campaignfinished=\"0\""));
            let _ = fs::remove_dir_all(sandbox);
        }
    }

    #[test]
    fn saved_campaign_inventory_marks_unverified_slot_files_as_start_new() {
        let sandbox = std::env::temp_dir().join(format!("ccm-saved-resume-{}", Uuid::new_v4()));
        let save = sandbox.join("paintball/Saves/Unsaved/Campaign/Last Stand.SC2Save");
        fs::create_dir_all(save.parent().unwrap()).unwrap();
        fs::write(&save, b"opaque save").unwrap();

        let resumes = inspect_saved_campaign_resumes_at(&sandbox).unwrap();
        assert_eq!(resumes.len(), 1);
        assert_eq!(resumes[0].campaign_id, "paintball");
        assert_eq!(resumes[0].save_count, 1);
        assert!(resumes[0].latest_save.is_none());
        assert_eq!(resumes[0].unverified_save_count, 1);
        let _ = fs::remove_dir_all(sandbox);
    }

    #[test]
    fn pending_journal_recovers_a_profile_switch_before_any_game_file_changes() {
        let sandbox = std::env::temp_dir().join(format!("ccm-pending-profile-{}", Uuid::new_v4()));
        let game_dir = sandbox.join("game");
        let profile = sandbox.join("sc2-profile");
        let profile_base = sandbox.join("profiles");
        let staging = game_dir.join(".ccm-reborn/staging/profile-crash");
        fs::create_dir_all(profile.join("Banks")).unwrap();
        // Test-only portable profile-store root. The production equivalent is
        // ~/.ccm-reborn/profiles, which is explicitly whitelisted.
        fs::create_dir_all(profile_base.join("Banks")).unwrap();
        fs::create_dir_all(&staging).unwrap();
        fs::write(profile.join("Banks/ZCampaign.SC2Bank"), b"yuri-bank").unwrap();
        fs::write(
            profile.join("CampaignProgress.xml"),
            "<root>\n  <CampaignProgress id=\"Swarm\" tutorialfinished=\"1\" campaignfinished=\"1\" />\n</root>\n",
        )
        .unwrap();
        fs::create_dir_all(profile_base.join("abathur/Banks")).unwrap();
        fs::write(profile_base.join("abathur/Banks/ZCampaign.SC2Bank"), b"abathur-bank").unwrap();
        fs::write(
            profile_base.join("abathur/CampaignProgress.xml"),
            "<root>\n  <CampaignProgress id=\"Swarm\" tutorialfinished=\"0\" campaignfinished=\"0\" />\n</root>\n",
        )
        .unwrap();

        apply_profile_transition_at_with_hook(
            &game_dir,
            &profile,
            &profile_base,
            "Maps/Campaign/swarm",
            "Maps/Campaign/swarm",
            "yuri",
            &[PathBuf::from("Mods/YuriCampaign.SC2Mod")],
            "abathur",
            &[PathBuf::from("Mods/Abathur.SC2Mod")],
            &staging,
            |transaction, profile_roots| {
                write_pending_install_journal(
                    &game_dir,
                    &PendingInstallJournal {
                        format: 1,
                        profile_transaction: transaction.clone(),
                        profile_roots: profile_roots.to_vec(),
                        previous_install: None,
                        new_state: None,
                        completed: false,
                    },
                )
            },
        )
        .unwrap();
        assert_eq!(fs::read(profile.join("Banks/ZCampaign.SC2Bank")).unwrap(), b"abathur-bank");

        assert!(recover_interrupted_install(&game_dir).unwrap());
        assert_eq!(fs::read(profile.join("Banks/ZCampaign.SC2Bank")).unwrap(), b"yuri-bank");
        assert!(fs::read_to_string(profile.join("CampaignProgress.xml"))
            .unwrap()
            .contains("tutorialfinished=\"1\""));
        assert!(!pending_install_path(&game_dir).exists());
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
            author: "Test".into(),
            version: "1.0".into(),
            profile_dir: None,
            archive_source: archive_path.display().to_string(),
            sha256: sha256_file(&archive_path).unwrap(),
            package_size: None,
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
        let profile = Path::new("/profile");
        assert!(!is_user_loadable_campaign_save(profile, &profile.join("Saves/SwarmCampaignSave.SC2Save")));
        assert!(is_user_loadable_campaign_save(profile, &profile.join("Saves/Campaign/Back in the Saddle.SC2Save")));
        assert!(is_user_loadable_campaign_save(profile, &profile.join("Saves/Unsaved/Campaign/Back in the Saddle.SC2Save")));
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

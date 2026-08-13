#[tauri::command]
async fn migrate_legacy_profile(request: MigrateLegacyProfileRequest) -> Result<LegacyProfileMigration, String> {
    tauri::async_runtime::spawn_blocking(move || migrate_legacy_profile_blocking(request))
        .await
        .map_err(|error| format!("Legacy profile migration worker failed: {error}"))?
}

fn migrate_legacy_profile_blocking(request: MigrateLegacyProfileRequest) -> Result<LegacyProfileMigration, String> {
    validate_campaign_id(&request.campaign_id)?;
    let profile = select_starcraft_profile(Some(&request.profile_dir))?
        .ok_or("Choose the SC2 account profile that owns this legacy snapshot.")?;
    let base = profile_store_base()?;
    validate_profile_store_chain(&base, &[])?;
    let account_store = profile_store_base_for(&profile.path)?;
    fs::create_dir_all(&account_store).map_err(io_error)?;
    let account_type = fs::symlink_metadata(&account_store).map_err(io_error)?.file_type();
    if account_type.is_symlink() || !account_type.is_dir() {
        return Err("The selected account's CCM profile store is not a regular directory.".into());
    }
    migrate_legacy_profile_store_at(&base, &account_store, &request.campaign_id)
}

fn migrate_legacy_profile_store_at(
    base: &Path,
    account_store: &Path,
    campaign_id: &str,
) -> Result<LegacyProfileMigration, String> {
    validate_campaign_id(campaign_id)?;
    let legacy = base.join(campaign_id);
    let legacy_type = fs::symlink_metadata(&legacy)
        .map_err(|_| format!("Legacy CCM profile snapshot {} was not found.", legacy.display()))?
        .file_type();
    if legacy_type.is_symlink() || !legacy_type.is_dir() {
        return Err("Legacy CCM profile snapshot must be a regular directory.".into());
    }
    let mut manifest = read_campaign_profile_resume_manifest(&legacy.join("ccm-resume.json"), campaign_id)?;
    let target = profile_store_root(account_store, campaign_id);
    if target.exists() {
        return Err(format!("The selected account already has a CCM profile for {campaign_id}; it was not overwritten."));
    }
    let files_copied = collect_regular_files(&legacy)?.len();
    let temporary = account_store.join(format!(".{campaign_id}.migrating-{}", Uuid::new_v4()));
    let result = (|| {
        copy_regular_tree(&legacy, &temporary)?;
        manifest.legacy_migrated = true;
        write_json_atomic(&temporary.join("ccm-resume.json"), &manifest)?;
        atomic_replace(&temporary, &target)
    })();
    if result.is_err() && temporary.exists() {
        let _ = fs::remove_dir_all(&temporary);
    }
    result?;
    Ok(LegacyProfileMigration { campaign_id: campaign_id.into(), files_copied })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_copies_a_legacy_snapshot_without_removing_its_rollback_copy() {
        let root = std::env::temp_dir().join(format!("ccm-legacy-migration-{}", Uuid::new_v4()));
        let base = root.join(".ccm-reborn/profiles");
        let legacy = base.join("paintball");
        let account = base.join("accounts/test-account");
        fs::create_dir_all(legacy.join("Banks")).unwrap();
        fs::create_dir_all(&account).unwrap();
        fs::write(legacy.join("Banks/ZCampaign.SC2Bank"), b"legacy-bank").unwrap();
        write_json_atomic(
            &legacy.join("ccm-resume.json"),
            &CampaignProfileResumeManifest {
                format: 1,
                campaign_id: "paintball".into(),
                target_path: "Maps/Campaign/swarm".into(),
                dependency_names: vec!["paintball.sc2mod".into()],
                captured_at: 123,
                legacy_migrated: false,
            },
        )
        .unwrap();

        let result = migrate_legacy_profile_store_at(&base, &account, "paintball").unwrap();
        let imported = account.join("paintball");
        assert_eq!(result.files_copied, 2);
        assert_eq!(fs::read(imported.join("Banks/ZCampaign.SC2Bank")).unwrap(), b"legacy-bank");
        assert!(read_campaign_profile_resume_manifest(&imported.join("ccm-resume.json"), "paintball").unwrap().legacy_migrated);
        assert!(legacy.is_dir(), "the old snapshot remains a rollback copy");
        let _ = fs::remove_dir_all(root);
    }
}

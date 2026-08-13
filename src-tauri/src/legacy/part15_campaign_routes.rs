fn campaign_roots_for_destinations<'a>(
    target_path: &str,
    destinations: impl IntoIterator<Item = &'a Path>,
) -> Result<Vec<PathBuf>, String> {
    let target = safe_campaign_target(target_path)?;
    let mut roots = vec![target.clone()];
    for destination in destinations {
        if destination.starts_with("Mods") {
            continue;
        }
        let root = campaign_root_for_destination(&target, destination)?;
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    Ok(roots)
}

fn campaign_roots_for_package_files(target_path: &str, files: &[PackageFilePlan]) -> Result<Vec<PathBuf>, String> {
    let destinations = files
        .iter()
        .map(|file| safe_relative_path(&file.destination))
        .collect::<Result<Vec<_>, _>>()?;
    campaign_roots_for_destinations(target_path, destinations.iter().map(PathBuf::as_path))
}

fn campaign_roots_for_staged_files(target_path: &str, files: &[StagedFile]) -> Result<Vec<PathBuf>, String> {
    let destinations = files
        .iter()
        .map(|file| safe_relative_path(&file.destination))
        .collect::<Result<Vec<_>, _>>()?;
    campaign_roots_for_destinations(target_path, destinations.iter().map(PathBuf::as_path))
}

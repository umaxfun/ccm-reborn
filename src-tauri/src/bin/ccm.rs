//! Standalone CLI for CCM Reborn.
//!
//! This binary intentionally has no automatic StarCraft II discovery. `plan`
//! and `install` delegate to the same backend paths as the Tauri commands;
//! `summary`/`inspect` inventory only an explicitly supplied directory.

use ccm_reborn_lib::{
    atomic_replace, cli_fixture_summary_json, cli_install_json, cli_installed_manifest_json,
    cli_local_add_json, cli_local_inspect_json, cli_local_list_json, cli_local_remove_json,
    cli_plan_json, cli_restore_json,
};
use ccm_reborn_lib::profile_core::{
    compare_snapshot, sort_progress_summaries, CampaignProgressSummary, FileDigest, ProfileIdentity,
    read_manifest,
};
use std::{
    collections::BTreeMap,
    env,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process,
};
use uuid::Uuid;

const HELP: &str = r#"CCM Reborn CLI

USAGE:
  ccm plan --game-dir PATH --archive PATH --campaign-id ID --title TITLE --sha256 HEX [--profile-dir PATH] [--output PATH]
  ccm install --game-dir PATH --archive PATH --campaign-id ID --title TITLE --sha256 HEX --confirm APPLY --profile-dir PATH [--author NAME] [--version V] [--output PATH]
  ccm restore --game-dir PATH --target PATH --confirm RESTORE --profile-dir PATH [--output PATH]
  ccm installed --game-dir PATH --target PATH [--output PATH]
  ccm summary --root PATH [--output PATH]
  ccm inspect --root PATH [--output PATH]
  ccm profile-key --family ID --major N --campaign-id ID
  ccm sort-summary --input FILE [--output PATH]
  ccm roundtrip-check --root PATH --manifest FILE [--output PATH]
  ccm local-inspect --archive PATH [--output PATH]
  ccm local-add --archive PATH [--store-dir PATH] [--title T] [--author A] [--version V] [--output PATH]
  ccm local-list [--store-dir PATH] [--output PATH]
  ccm local-remove --id ID [--store-dir PATH] [--output PATH]
  ccm --help

COMMANDS:
  plan     Run the read-only campaign install planner and emit its full JSON plan.
  install  Apply a package after an explicit `--confirm APPLY` acknowledgement.
  restore  Restore one explicit campaign branch and its saved vanilla profile.
  installed  Show the exact package file inventory recorded for a campaign branch.
  summary  Inventory an explicit directory (paths, kinds, sizes, SHA-256).
  inspect  Alias for summary, useful in scripts and diagnostics.
  profile-key  Validate and print a stable family/major profile identity.
  sort-summary Sort progress summaries by branch-local progress recency rules.
  roundtrip-check Compare an explicit root against manifest file hashes.

NOTES:
  The game directory is always explicit; the CLI never searches for one.
  `plan` and `installed` are read-only. `install` and `restore` mutate only
  the supplied game directory and the selected StarCraft II profile, and
  require StarCraft II closed.
"#;

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        process::exit(2);
    }
}

fn run() -> Result<(), String> {
    run_arguments(env::args().skip(1).collect())
}

fn run_arguments(mut arguments: Vec<String>) -> Result<(), String> {
    if arguments.is_empty() || arguments.iter().any(|argument| argument == "--help" || argument == "-h") {
        print!("{HELP}");
        return Ok(());
    }

    let command = arguments.remove(0);
    if command == "help" {
        print!("{HELP}");
        return Ok(());
    }
    let options = parse_options(&arguments)?;
    match command.as_str() {
        "plan" => {
            let value = cli_plan_json(
                required(&options, "campaign-id")?,
                required(&options, "title")?,
                required(&options, "archive")?,
                required(&options, "sha256")?,
                required(&options, "game-dir")?,
                options.get("profile-dir").cloned(),
            )?;
            emit_json(value, options.get("output"))
        }
        "install" => {
            if required(&options, "confirm")? != "APPLY" {
                return Err("Refusing install: pass --confirm APPLY after reviewing a dry-run.".into());
            }
            let value = cli_install_json(
                required(&options, "campaign-id")?,
                required(&options, "title")?,
                options.get("author").cloned().unwrap_or_default(),
                options.get("version").cloned().unwrap_or_default(),
                required(&options, "archive")?,
                required(&options, "sha256")?,
                required(&options, "game-dir")?,
                options.get("profile-dir").cloned(),
            )?;
            emit_json(value, options.get("output"))
        }
        "restore" => {
            if required(&options, "confirm")? != "RESTORE" {
                return Err("Refusing restore: pass --confirm RESTORE after reviewing the active install.".into());
            }
            emit_json(
                cli_restore_json(
                    required(&options, "game-dir")?,
                    options.get("profile-dir").cloned(),
                    required(&options, "target")?,
                )?,
                options.get("output"),
            )
        }
        "installed" => {
            let root = PathBuf::from(required(&options, "game-dir")?);
            let value = cli_installed_manifest_json(&root, required(&options, "target")?.as_str())?;
            emit_json(value, options.get("output"))
        }
        "summary" | "inspect" => {
            let root = PathBuf::from(required(&options, "root")?);
            let value = cli_fixture_summary_json(&root)?;
            emit_json(value, options.get("output"))
        }
        "profile-key" => {
            let identity = ProfileIdentity::new(
                required(&options, "family")?.as_str(),
                required(&options, "major")?
                    .parse()
                    .map_err(|_| "--major must be a positive integer".to_string())?,
                required(&options, "campaign-id")?.as_str(),
            )?;
            emit_json(serde_json::to_value(identity).map_err(|error| error.to_string())?, options.get("output"))
        }
        "sort-summary" => {
            let input = PathBuf::from(required(&options, "input")?);
            let text = fs::read_to_string(&input).map_err(io_error)?;
            let mut summaries = serde_json::from_str::<Vec<CampaignProgressSummary>>(&text)
                .or_else(|_| {
                    serde_json::from_str::<serde_json::Value>(&text)
                        .and_then(|value| serde_json::from_value(value["profiles"].clone()))
                })
                .map_err(|error| format!("invalid progress summary JSON: {error}"))?;
            sort_progress_summaries(&mut summaries);
            emit_json(serde_json::to_value(summaries).map_err(|error| error.to_string())?, options.get("output"))
        }
        "roundtrip-check" => {
            let root = PathBuf::from(required(&options, "root")?);
            let manifest = read_manifest(&PathBuf::from(required(&options, "manifest")?))?;
            let expected = manifest
                .files
                .iter()
                .map(|file| FileDigest {
                    relative_path: file.relative_path.clone(),
                    size: file.size,
                    sha256: file.sha256.clone(),
                })
                .collect::<Vec<_>>();
            let report = compare_snapshot(&root, &expected)?;
            let equal = report.equal;
            emit_json(serde_json::to_value(report).map_err(|error| error.to_string())?, options.get("output"))?;
            if equal { Ok(()) } else { Err("round-trip check failed".into()) }
        }
        "local-inspect" => {
            let value = cli_local_inspect_json(required(&options, "archive")?)?;
            emit_json(value, options.get("output"))
        }
        "local-add" => {
            let value = cli_local_add_json(
                options.get("store-dir").cloned(),
                required(&options, "archive")?,
                options.get("title").cloned(),
                options.get("author").cloned(),
                options.get("version").cloned(),
            )?;
            emit_json(value, options.get("output"))
        }
        "local-list" => {
            let value = cli_local_list_json(options.get("store-dir").cloned())?;
            emit_json(value, options.get("output"))
        }
        "local-remove" => {
            let value = cli_local_remove_json(options.get("store-dir").cloned(), required(&options, "id")?)?;
            emit_json(value, options.get("output"))
        }
        "--version" | "-V" => {
            println!("ccm-reborn {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        _ => Err(format!("unknown command {command:?}; run `ccm --help`")),
    }
}

fn parse_options(arguments: &[String]) -> Result<BTreeMap<String, String>, String> {
    let mut options = BTreeMap::new();
    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];
        let Some(name) = argument.strip_prefix("--") else {
            return Err(format!("unexpected argument {argument:?}; options use --name value"));
        };
        if name.is_empty() {
            return Err("empty option name".into());
        }
        let value = arguments.get(index + 1).ok_or_else(|| format!("missing value for --{name}"))?;
        if value.starts_with('-') {
            return Err(format!("missing value for --{name}"));
        }
        if options.insert(name.to_string(), value.to_string()).is_some() {
            return Err(format!("option --{name} was provided more than once"));
        }
        index += 2;
    }
    Ok(options)
}

fn required(options: &BTreeMap<String, String>, name: &str) -> Result<String, String> {
    options
        .get(name)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or_else(|| format!("missing required --{name}; run `ccm --help`"))
}

fn emit_json(value: serde_json::Value, output: Option<&String>) -> Result<(), String> {
    let rendered = serde_json::to_string_pretty(&value)
        .map_err(|error| format!("could not encode JSON: {error}"))?;
    let Some(output) = output else {
        println!("{rendered}");
        return Ok(());
    };
    write_atomic(Path::new(output), rendered.as_bytes())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err("output path is empty".into());
    }
    if path.exists() && fs::symlink_metadata(path).map_err(io_error)?.file_type().is_symlink() {
        return Err("refusing to overwrite a symbolic link".into());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    let temporary = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    let mut file = File::create(&temporary).map_err(io_error)?;
    file.write_all(bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    atomic_replace(&temporary, path)
}

fn io_error(error: std::io::Error) -> String {
    format!("filesystem operation failed: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_mentions_read_only_commands() {
        assert!(HELP.contains("plan"));
        assert!(HELP.contains("install"));
        assert!(HELP.contains("installed"));
        assert!(HELP.contains("summary"));
        assert!(HELP.contains("roundtrip-check"));
        assert!(HELP.contains("never searches for one"));
    }

    #[test]
    fn summary_smoke_uses_only_explicit_fixture_root() {
        let root = env::temp_dir().join(format!("ccm-cli-fixture-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("Banks")).unwrap();
        fs::create_dir_all(root.join("Saves/Campaign")).unwrap();
        fs::write(root.join("Banks/ZCampaign.SC2Bank"), b"bank").unwrap();
        fs::write(root.join("CampaignProgress.xml"), b"<CampaignProgress />").unwrap();
        fs::write(root.join("Saves/Campaign/Quick Save.SC2Save"), b"save").unwrap();

        let summary = cli_fixture_summary_json(&root).unwrap();
        assert_eq!(summary["schemaVersion"], 1);
        assert_eq!(summary["fileCount"], 3);
        assert!(summary["files"].as_array().unwrap().iter().any(|file| {
            file["kind"] == "campaign-bank" && file["relativePath"] == "Banks/ZCampaign.SC2Bank"
        }));
        assert!(summary["files"].as_array().unwrap().iter().any(|file| {
            file["kind"] == "campaign-save" && file["relativePath"] == "Saves/Campaign/Quick Save.SC2Save"
        }));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn install_refuses_without_the_explicit_confirmation_token() {
        let error = run_arguments(vec![
            "install".into(),
            "--confirm".into(),
            "no".into(),
        ])
        .unwrap_err();
        assert!(error.contains("--confirm APPLY"));
    }

    #[test]
    fn install_command_uses_the_shared_mutating_core() {
        use sha2::{Digest, Sha256};
        use zip::{write::SimpleFileOptions, ZipWriter};

        let root = env::temp_dir().join(format!("ccm-cli-install-{}", Uuid::new_v4()));
        let game = root.join("game");
        let archive = root.join("package.zip");
        let output = root.join("result.json");
        fs::create_dir_all(&game).unwrap();
        let file = File::create(&archive).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        zip.start_file("CLI Test/metadata.txt", options).unwrap();
        zip.write_all(b"title=CLI Test\nauthor=Test\ncampaign=WoL\nversion=1\n").unwrap();
        zip.start_file("CLI Test/mission.SC2Map", options).unwrap();
        zip.write_all(b"cli mission").unwrap();
        zip.finish().unwrap();
        let hash = {
            let mut file = File::open(&archive).unwrap();
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut file, &mut bytes).unwrap();
            hex::encode(Sha256::digest(bytes))
        };

        run_arguments(vec![
            "install".into(),
            "--game-dir".into(), game.display().to_string(),
            "--archive".into(), archive.display().to_string(),
            "--campaign-id".into(), "cli-test".into(),
            "--title".into(), "CLI Test".into(),
            "--sha256".into(), hash,
            "--confirm".into(), "APPLY".into(),
            "--output".into(), output.display().to_string(),
        ])
        .unwrap();
        assert_eq!(fs::read(game.join("Maps/Campaign/mission.SC2Map")).unwrap(), b"cli mission");
        let result = fs::read_to_string(&output).unwrap();
        assert!(result.contains("cli-test"));
        let restore_output = root.join("restore.json");
        run_arguments(vec![
            "restore".into(),
            "--game-dir".into(), game.display().to_string(),
            "--target".into(), "Maps/Campaign".into(),
            "--confirm".into(), "RESTORE".into(),
            "--output".into(), restore_output.display().to_string(),
        ])
        .unwrap();
        assert!(!game.join("Maps/Campaign/mission.SC2Map").exists());
        assert!(fs::read_to_string(restore_output).unwrap().contains("restoredFiles"));
        let _ = fs::remove_dir_all(root);
    }
}

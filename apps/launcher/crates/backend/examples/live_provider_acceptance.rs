//! Opt-in provider checks in a marked, newly authorized acceptance profile.
//! Reports contain only outcomes; account identifiers and tokens are never printed.
use anyhow::{ensure, Context};
use industrialis_backend::{
    accounts, backup_manager::BackupManager, backups, google_drive::GoogleDrive,
    paths::LauncherPaths, profile_owner, settings,
};
use std::{io::Write, path::PathBuf, sync::Arc};

fn main() {
    if industrialis_backend::game_broker::run_if_requested() {
        return;
    }
    if let Err(error) = run() {
        // Display the outer stage only: an HTTP error chain may contain credentials.
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        (args.len() == 2 && ["microsoft", "google"].contains(&args[1].as_str()))
            || (args.len() == 4 && args[1] == "microsoft-game"),
        "Usage: live_provider_acceptance <marked-profile> microsoft|google; or <marked-profile> microsoft-game <instance-id> <java-path>"
    );
    let profile = PathBuf::from(&args[0]);
    ensure!(profile.is_absolute(), "Acceptance profile must be absolute");
    ensure!(
        !std::fs::symlink_metadata(&profile)?
            .file_type()
            .is_symlink(),
        "Acceptance profile cannot be redirected"
    );
    ensure!(
        std::fs::read_to_string(profile.join(".gpui-live-accounts"))
            .ok()
            .as_deref()
            == Some("industrialis-gpui-expendable-live-accounts-v1"),
        "Use a marked expendable account profile"
    );
    let paths = LauncherPaths::new(profile);
    let _owner = match profile_owner::acquire(&paths, profile_owner::OwnerKind::Operation)? {
        profile_owner::Acquisition::Primary(owner) => owner,
        _ => anyhow::bail!("Acceptance profile is already in use"),
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        match args[1].as_str() {
            "microsoft" => microsoft(&paths)
                .await
                .context("Microsoft acceptance failed"),
            "google" => google(&paths).await.context("Google acceptance failed"),
            "microsoft-game" => microsoft_game(&paths, &args[2], &args[3])
                .await
                .context("Authenticated client acceptance failed"),
            _ => anyhow::bail!("Provider must be microsoft or google"),
        }
    })
}

async fn microsoft_game(paths: &LauncherPaths, id: &str, java: &str) -> anyhow::Result<()> {
    use industrialis_backend::{launch, launch_config};
    use std::{
        sync::atomic::{AtomicBool, Ordering},
        time::Duration,
    };
    let id = industrialis_backend::paths::validate_instance_id(id)?;
    let instance = paths.instance_dir(&id)?;
    ensure!(
        !tokio::fs::symlink_metadata(&instance)
            .await?
            .file_type()
            .is_symlink(),
        "Acceptance instance cannot be redirected"
    );
    ensure!(
        !launch::running_instances(paths)
            .await?
            .iter()
            .any(|(_, _, live)| *live),
        "Acceptance profile already has a running game"
    );
    let stored = accounts::load_accounts(paths).await?;
    let account = stored
        .iter()
        .find(|a| a.account_type == "msa")
        .context("Authorize the expendable Microsoft profile first")?;
    ensure!(
        account
            .minecraft_entitlement
            .as_ref()
            .is_some_and(|e| e.owns_minecraft && e.can_play_minecraft),
        "Test account has no playable license"
    );
    let mut config = settings::load_instance_settings(paths, &id).await?;
    let before = config.total_play_seconds;
    config.override_account = true;
    config.account_id = Some(account.id.clone());
    config.override_java_location = true;
    config.java_path = Some(java.into());
    config.override_memory = true;
    config.min_ram_mb = 1024;
    config.max_ram_mb = 6144;
    config.perm_gen_mb = 0;
    config.override_window = true;
    config.window_width = 854;
    config.window_height = 480;
    config.launch_maximized = false;
    config.join_server_on_launch = false;
    config.override_commands = false;
    config.override_env = false;
    config.override_game_time = true;
    config.record_game_time = true;
    settings::save_instance_settings(paths, &id, &config).await?;
    let ready = Arc::new(AtomicBool::new(false));
    let observed = ready.clone();
    // The normal backend persists logs; this harness prints no authenticated game output.
    let emit: launch_config::EmitLaunchLog = Arc::new(move |_, _, line| {
        if line.contains("Forge Mod Loader has successfully loaded") {
            observed.store(true, Ordering::SeqCst);
        }
    });
    let game_paths = paths.clone();
    let game_id = id.clone();
    let mut task =
        tokio::spawn(async move { launch::launch_instance(&game_paths, &game_id, emit).await });
    let start = std::time::Instant::now();
    let limit = std::env::var("GPUI_GAME_ACCEPTANCE_SECONDS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(900);
    loop {
        tokio::select! {
            result = &mut task => { result??; anyhow::bail!("Authenticated client exited before deliberate Stop"); }
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                if ready.load(Ordering::SeqCst) { break; }
                if start.elapsed().as_secs() >= limit {
                    let _ = launch::kill_instance(paths, &id).await;
                    let _ = tokio::time::timeout(Duration::from_secs(30), task).await;
                    anyhow::bail!("Timed out before authenticated Forge loading completed");
                }
                if start.elapsed().as_secs() % 30 == 0 { println!("Waiting for authenticated Forge loading: {} seconds", start.elapsed().as_secs()); }
            }
        }
    }
    ensure!(
        launch::kill_instance(paths, &id).await?,
        "Authenticated client Stop failed"
    );
    if let Err(error) = tokio::time::timeout(Duration::from_secs(30), task).await?? {
        ensure!(
            error.to_string().contains("game exited with code"),
            "Client shutdown failed"
        );
    }
    ensure!(
        !launch::running_instances(paths)
            .await?
            .iter()
            .any(|(_, _, live)| *live),
        "Client remains live after Stop"
    );
    ensure!(
        accounts::load_accounts(paths)
            .await?
            .iter()
            .any(|a| a.id == account.id && a.account_type == "msa"),
        "Authorized account did not persist"
    );
    let play = settings::load_instance_settings(paths, &id)
        .await?
        .total_play_seconds;
    ensure!(
        play > before,
        "Authenticated session play time was not recorded"
    );
    report(paths, &format!("microsoft-game-{id}"), serde_json::json!({"date":chrono::Utc::now().to_rfc3339(),
        "platform":std::env::consts::OS,"arch":std::env::consts::ARCH,"instance":id,
        "existing_authorized_test_account":true,"real_authenticated_launch":true,"forge_mod_loading_complete":true,
        "deliberate_stop":true,"account_persisted":true,"play_seconds_added":play-before,"elapsed_seconds":start.elapsed().as_secs(),
        "original_profile":"untouched","desktop":"virtual if invoked in the isolated Linux fixture; no world gameplay or server authentication"})).await
}

async fn report(
    paths: &LauncherPaths,
    provider: &str,
    result: serde_json::Value,
) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(&result)?;
    tokio::fs::write(
        paths.base().join(format!("{provider}-acceptance.json")),
        &bytes,
    )
    .await?;
    println!("{}", String::from_utf8(bytes)?);
    Ok(())
}

async fn microsoft(paths: &LauncherPaths) -> anyhow::Result<()> {
    let stored = accounts::load_accounts(paths).await?;
    let account = stored
        .iter()
        .find(|account| account.account_type == "msa")
        .context("Complete Microsoft sign-in in this acceptance profile first")?;
    ensure!(
        account.minecraft_profile.is_some(),
        "Minecraft profile is missing"
    );
    ensure!(
        account
            .minecraft_entitlement
            .as_ref()
            .is_some_and(|e| e.owns_minecraft && e.can_play_minecraft),
        "The signed-in test account does not have a playable Minecraft license"
    );
    ensure!(
        account.skin_png_base64.is_some(),
        "Minecraft skin was not downloaded"
    );
    // Verify this exercises the refresh branch, rather than the cached-token
    // branch. No expiry or saved token is changed to force the request.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;
    ensure!(
        account
            .msa_token
            .as_ref()
            .is_some_and(|msa| msa.expires_at - now <= 12 * 60 * 60),
        "The test account is outside the normal Microsoft refresh window"
    );
    let token = accounts::ensure_fresh_token(paths, account).await?;
    ensure!(
        !token.is_empty(),
        "Microsoft refresh returned an empty game token"
    );
    let reloaded = accounts::load_accounts(paths).await?;
    let saved = reloaded
        .iter()
        .find(|value| value.id == account.id)
        .context("Refreshed account could not be reloaded")?;
    ensure!(
        saved
            .yggdrasil_token
            .as_ref()
            .is_some_and(|saved| saved.token == token),
        "Refreshed game token was not persisted"
    );
    report(paths, "microsoft", serde_json::json!({
        "date":chrono::Utc::now().to_rfc3339(),"platform":std::env::consts::OS,"arch":std::env::consts::ARCH,
        "fresh_device_sign_in":true,"minecraft_profile":true,"playable_license":true,"skin_download":true,
        "refresh_chain":true,"same_account_reloaded":true,"refreshed_token_persisted":true,
        "original_profile":"untouched","real_game_launch":"not tested by this harness",
        "existing_original_account_compatibility":"not established by fresh acceptance sign-in"
    })).await
}

fn archive(path: &std::path::Path, bytes: usize) -> anyhow::Result<Vec<u8>> {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path)?);
    zip.start_file(
        "expendable-acceptance.bin",
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )?;
    // Generated data only, deliberately larger than one resumable upload chunk.
    let seed = uuid::Uuid::new_v4();
    let data: Vec<_> = (0..bytes)
        .map(|i| seed.as_bytes()[i % 16] ^ (i % 251) as u8)
        .collect();
    zip.write_all(&data)?;
    zip.finish()?;
    Ok(std::fs::read(path)?)
}

async fn google(paths: &LauncherPaths) -> anyhow::Result<()> {
    let initial = Arc::new(GoogleDrive::new(paths.clone()));
    let status = initial.status().await?;
    ensure!(
        status.configured,
        "Configure a Google desktop client ID first"
    );
    if !status.connected {
        println!("Complete Google authorization in your default browser.");
        ensure!(initial.connect().await?.connected, "Google did not connect");
    }
    drop(initial);
    // A new provider must decrypt the saved Windows credential and refresh its
    // access token without asking for another browser authorization.
    let drive = Arc::new(GoogleDrive::new(paths.clone()));
    ensure!(
        drive.status().await?.connected,
        "Google connection did not persist"
    );
    let id = format!("GpuiCloudAcceptance{}", uuid::Uuid::new_v4().simple());
    let directory = paths.instance_dir(&id)?;
    let backup_dir = paths.instance_backups_dir(&id)?;
    tokio::fs::create_dir_all(&backup_dir).await?;
    tokio::fs::write(directory.join("mmc-pack.json"), b"{}").await?;
    let mut config = settings::load_instance_settings(paths, &id).await?;
    config.name = "Expendable cloud acceptance".into();
    config.backups_enabled = true;
    config.backup_retention_override = Some(1);
    settings::save_instance_settings(paths, &id, &config).await?;
    ensure!(
        backups::list_remote(&id, drive.as_ref()).await?.is_empty(),
        "New acceptance instance already has cloud data"
    );
    let first_name = "interrupted.zip";
    let first_bytes = archive(&backup_dir.join(first_name), 10 * 1024 * 1024)?;
    let first = backups::prepare(paths, &id, first_name, None).await?;
    let acknowledged = Arc::new(tokio::sync::Notify::new());
    let signal = acknowledged.clone();
    let progress: backups::Progress = Arc::new(move |done, total| {
        if done >= 8 * 1024 * 1024 && done < total {
            signal.notify_one();
        }
    });
    // Cancel only this test upload after Google's acknowledgement of a partial
    // chunk. A fresh queue delivery below retries the uncommitted backup.
    {
        let upload = backups::upload(&first, drive.as_ref(), progress);
        tokio::pin!(upload);
        tokio::select! {
            result = &mut upload => { result?; anyhow::bail!("Upload completed before the interruption check"); }
            _ = acknowledged.notified() => {}
        }
    }
    ensure!(
        backups::list_remote(&id, drive.as_ref()).await?.is_empty(),
        "Interrupted upload published a manifest"
    );
    println!("Google acknowledged a partial upload; checking durable retry.");
    let manager = BackupManager::new(paths.clone(), drive.clone());
    manager.upload_now(&id, first_name).await?;
    drop(manager);
    let manager = BackupManager::new(paths.clone(), drive.clone());
    ensure!(
        manager
            .list(&id)
            .await?
            .iter()
            .any(|entry| entry.summary.snapshot_id == first.summary.snapshot_id),
        "Pending upload did not survive manager restart"
    );
    manager
        .retry(Some(id.clone()), Some(first.summary.snapshot_id.clone()))
        .await?;
    manager.run_once().await?;
    let uploaded = backups::list_remote(&id, drive.as_ref()).await?;
    ensure!(
        uploaded.len() == 1 && uploaded[0].snapshot_id == first.summary.snapshot_id,
        "Retry did not commit exactly one backup"
    );
    let downloaded = manager.download(&id, &first.summary.snapshot_id).await?;
    ensure!(
        tokio::fs::read(backup_dir.join(&downloaded.file_name)).await? == first_bytes,
        "Restored backup bytes differ"
    );
    ensure!(
        tokio::fs::read(backup_dir.join(first_name)).await? == first_bytes,
        "Restore replaced the existing local archive"
    );
    let second_name = "retention.zip";
    let second_bytes = archive(&backup_dir.join(second_name), 32768)?;
    let second = backups::prepare(paths, &id, second_name, None).await?;
    manager.upload_now(&id, second_name).await?;
    manager.run_once().await?;
    let retained = backups::list_remote(&id, drive.as_ref()).await?;
    ensure!(
        retained.len() == 1 && retained[0].snapshot_id == second.summary.snapshot_id,
        "Retention did not preserve only the newest test backup"
    );
    manager.delete(&id, &second.summary.snapshot_id).await?;
    ensure!(
        backups::list_remote(&id, drive.as_ref()).await?.is_empty(),
        "Test cloud files remain after deletion"
    );
    ensure!(
        tokio::fs::read(backup_dir.join(first_name)).await? == first_bytes
            && tokio::fs::read(backup_dir.join(second_name)).await? == second_bytes,
        "Cloud retention/deletion changed local test archives"
    );
    report(paths, "google", serde_json::json!({
        "date":chrono::Utc::now().to_rfc3339(),"platform":std::env::consts::OS,"arch":std::env::consts::ARCH,
        "encrypted_connection_reloaded":true,"access_token_refreshed_without_reconnect":true,
        "partial_upload_acknowledged_and_cancelled":true,"queue_survived_manager_restart":true,
        "retry_committed_once":true,"download_exact_bytes":true,"restore_preserved_local_archive":true,
        "retention_scoped_to_unique_test_instance":true,"test_remote_files_deleted":true,
        "local_archives_preserved":true,"test_instance":id,"original_profile":"untouched",
        "boundary":"backend/provider acceptance; physical GPUI sign-in controls not verified"
    })).await
}

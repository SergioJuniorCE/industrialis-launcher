//! Headless harness for the Rust backend (Phase 2).
//!
//! Every command exercises a backend service without the GPUI window, so
//! services are verified before UI wiring lands:
//!
//! ```sh
//! industrialis-cli detect-java
//! industrialis-cli test-java --path "C:/Java/bin/java.exe"
//! industrialis-cli list-instances
//! industrialis-cli show-settings [--instance <id>]
//! industrialis-cli list-accounts
//! industrialis-cli create-offline-account Steve
//! industrialis-cli ms-login
//! ```
//!
//! `--data-dir` overrides the OS app-data location (also settable via
//! `INDUSTRIALIS_DATA_DIR`).

use clap::{Parser, Subcommand};
use industrialis_backend::{
    accounts, install, instances, java, launch, launch_config, pack_files, paths::LauncherPaths,
    settings,
};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Parser)]
#[command(name = "industrialis-cli", about = "Industrialis backend harness")]
struct Cli {
    /// Override the launcher data dir (default: OS app-data location).
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Detect installed Java runtimes (mirrors `invoke("detect_java")`).
    DetectJava,
    /// Test a Java executable (mirrors `invoke("test_java")`).
    TestJava {
        /// Explicit executable; defaults to JAVA_HOME/PATH resolution.
        #[arg(long)]
        path: Option<String>,
    },
    /// List discovered instances (mirrors `invoke("get_instances")`).
    ListInstances,
    /// Read the existing profile in place; print counts only, without writing files.
    VerifyProfile,
    /// Print launcher settings, or one instance's settings.
    ShowSettings {
        #[arg(long)]
        instance: Option<String>,
    },
    /// List stored accounts (mirrors `invoke("get_accounts")`).
    ListAccounts,
    /// Create an offline account (mirrors `invoke("add_offline_account")`).
    CreateOfflineAccount { username: String },
    /// Microsoft device-code login (mirrors `invoke("start_microsoft_login")`).
    MsLogin,
    /// Launch an instance and stream its logs (mirrors `invoke("launch_instance")`).
    Launch {
        id: String,
        /// Resolve the launch command without spawning the game.
        #[arg(long)]
        dry_run: bool,
    },
    /// List tracked game processes with liveness.
    Ps,
    /// Kill a running instance (mirrors `invoke("kill_instance")`).
    Kill { id: String },
    /// Download + install a pack version (mirrors `invoke("download_install")`).
    Install {
        id: String,
        #[arg(long)]
        pack_version: String,
        #[arg(long, default_value = "java17+")]
        java_type: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        group: Option<String>,
    },
    /// Preview a pack update's mod changes (mirrors `invoke("preview_update_mods")`).
    PreviewUpdate {
        id: String,
        #[arg(long)]
        pack_version: String,
        #[arg(long)]
        java_type: Option<String>,
    },
    /// Update an instance to another pack version (mirrors `invoke("update_instance")`).
    Update {
        id: String,
        #[arg(long)]
        pack_version: String,
        #[arg(long)]
        java_type: Option<String>,
        /// Custom-mod identities to keep (repeatable).
        #[arg(long)]
        keep_mod: Vec<String>,
    },
    /// Clean-reinstall an instance (mirrors `invoke("reinstall_instance")`).
    Reinstall {
        id: String,
        #[arg(long)]
        pack_version: String,
        #[arg(long)]
        java_type: Option<String>,
    },
    /// List pack versions from the GTNH catalog.
    PackVersions,
    /// Check only GPUI launcher releases (does not install or write data).
    CheckLauncherUpdate {
        #[arg(long)]
        current_version: String,
    },
    /// Refresh cached instance sizes.
    RefreshSizes { ids: Vec<String> },
    /// Read the retained bundled configuration preset's overlay status.
    PresetStatus {
        id: String,
        #[arg(long, default_value = "aditya")]
        preset: String,
    },
    /// Enable a retained preset, or remove its overlays while keeping game files.
    ApplyPreset {
        id: String,
        #[arg(long, default_value = "aditya")]
        preset: String,
        #[arg(long)]
        disable: bool,
    },
}

impl Command {
    fn writes_profile(&self) -> bool {
        !matches!(
            self,
            Self::DetectJava
                | Self::TestJava { .. }
                | Self::ListInstances
                | Self::VerifyProfile
                | Self::ShowSettings { .. }
                | Self::ListAccounts
                | Self::PackVersions
                | Self::CheckLauncherUpdate { .. }
                | Self::PresetStatus { .. }
        )
    }
}

fn print_json(value: &serde_json::Value) {
    println!("{}", serde_json::to_string_pretty(value).unwrap());
}

fn main() -> Result<(), anyhow::Error> {
    if industrialis_backend::game_broker::run_if_requested() {
        return Ok(());
    }
    let cli = Cli::parse();
    let paths = match cli.data_dir {
        Some(dir) => LauncherPaths::new(dir),
        None => LauncherPaths::system(),
    };
    // Read-only inspection remains available while a launcher owns the profile.
    // Every command with possible writes holds ownership through completion.
    let _owner = if cli.command.writes_profile() {
        use industrialis_backend::profile_owner::{acquire, Acquisition, OwnerKind};
        match acquire(&paths, OwnerKind::Operation)? {
            Acquisition::Primary(owner) => Some(owner),
            Acquisition::Existing(_) => anyhow::bail!("This profile is in use. Close its launcher or wait for the current command before making changes."),
        }
    } else {
        None
    };
    // Runtime teardown (including blocking work) precedes guard release.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(run_command(cli.command, paths))
}

async fn run_command(command: Command, paths: LauncherPaths) -> Result<(), anyhow::Error> {
    match command {
        Command::DetectJava => {
            let list = java::detect_java().await;
            print_json(&serde_json::to_value(&list)?);
        }
        Command::PresetStatus { id, preset } => {
            let enabled = industrialis_backend::config_presets::get_config_preset_status(
                &paths, &id, &preset,
            )
            .await?;
            print_json(&serde_json::json!({"preset":preset,"enabled":enabled}));
        }
        Command::ApplyPreset {
            id,
            preset,
            disable,
        } => {
            industrialis_backend::config_presets::apply_config_preset(
                &paths, &id, &preset, !disable,
            )
            .await?;
            print_json(&serde_json::json!({"preset":preset,"enabled":!disable}));
        }
        Command::TestJava { path } => {
            println!("{}", java::test_java(path.as_deref()).await?);
        }
        Command::ListInstances => {
            let list = instances::load_instances(&paths).await?;
            print_json(&serde_json::to_value(&list)?);
        }
        Command::VerifyProfile => {
            let _settings = settings::load_launcher_settings(&paths).await?;
            let (accounts, account_issues) = accounts::load_accounts_with_issues(&paths).await?;
            let (instances, instance_issues) =
                instances::load_instances_with_issues(&paths).await?;
            print_json(&serde_json::json!({
                "data_dir": paths.base(),
                "settings_readable": true,
                "accounts": accounts.len(),
                "unrecognized_account_rows": account_issues,
                "instances": instances.len(),
                "instance_read_errors": instance_issues.len(),
            }));
            if account_issues > 0 || !instance_issues.is_empty() {
                anyhow::bail!(
                    "Some profile entries could not be interpreted; no files were changed."
                );
            }
        }
        Command::ShowSettings { instance } => match instance {
            Some(id) => print_json(&serde_json::to_value(
                &settings::load_instance_settings(&paths, &id).await?,
            )?),
            None => print_json(&serde_json::to_value(
                &settings::load_launcher_settings(&paths).await?,
            )?),
        },
        Command::ListAccounts => {
            let list = accounts::load_accounts(&paths).await?;
            print_json(&serde_json::to_value(
                &list
                    .iter()
                    .map(accounts::account_to_info)
                    .collect::<Vec<_>>(),
            )?);
        }
        Command::CreateOfflineAccount { username } => {
            let info = accounts::create_offline_account(&paths, &username).await?;
            print_json(&serde_json::to_value(&info)?);
        }
        Command::MsLogin => {
            let info = accounts::start_microsoft_login(&paths, |code| {
                println!(
                    "Open {} and enter code: {}",
                    code.verification_uri, code.user_code
                );
                println!("{}", code.message);
            })
            .await?;
            print_json(&serde_json::to_value(&info)?);
        }
        Command::Launch { id, dry_run } => {
            if dry_run {
                let instance = paths.instance_dir(&id)?;
                let emit: launch_config::EmitLaunchLog =
                    Arc::new(|_, stream, line| println!("[{stream}] {line}"));
                let settings = settings::load_instance_settings(&paths, &id).await?;
                let launcher = settings::load_launcher_settings(&paths).await?;
                // Same precedence as launch resolution: instance override,
                // launcher default, then PATH/JAVA_HOME detection.
                let java = settings
                    .java_path
                    .clone()
                    .filter(|_| settings.override_java_location)
                    .or(launcher.default_java_path.clone())
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| {
                        java::java_path()
                            .map(|p| p.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    });
                let config = launch_config::build_launch_config(&instance, &id, &emit).await?;
                print_json(&serde_json::json!({
                    "java": java,
                    "mainClass": config.main_class,
                    "minecraftVersion": config.minecraft_version,
                    "libraries": config.libraries.len(),
                    "jvmArgs": config.jvm_args,
                    "programArgs": config.program_args,
                }));
            } else {
                let emit: launch_config::EmitLaunchLog =
                    Arc::new(|_, stream, line| println!("[{stream}] {line}"));
                launch::launch_instance(&paths, &id, emit).await?;
            }
        }
        Command::Ps => {
            let running = launch::running_instances(&paths).await?;
            print_json(&serde_json::to_value(&running.iter().map(|(id, pid, live)| serde_json::json!({ "id": id, "pid": pid, "alive": live })).collect::<Vec<_>>())?);
        }
        Command::Kill { id } => {
            if launch::kill_instance(&paths, &id).await? {
                println!("killed {id}");
            } else {
                println!("{id} is not running");
            }
        }
        Command::Install {
            id,
            pack_version,
            java_type,
            name,
            group,
        } => {
            install::download_install(
                &paths,
                &id,
                &pack_version,
                &java_type,
                name.as_deref(),
                group.as_deref(),
                progress_emit(),
            )
            .await?;
            println!("installed {id} @ {pack_version}");
        }
        Command::PreviewUpdate {
            id,
            pack_version,
            java_type,
        } => {
            let preview = install::preview_update(
                &paths,
                &id,
                &pack_version,
                java_type.as_deref(),
                progress_emit(),
            )
            .await?;
            print_json(&serde_json::to_value(&preview)?);
        }
        Command::Update {
            id,
            pack_version,
            java_type,
            keep_mod,
        } => {
            install::update_instance(
                &paths,
                &id,
                &pack_version,
                java_type.as_deref(),
                &keep_mod,
                progress_emit(),
            )
            .await?;
            println!("updated {id} to {pack_version}");
        }
        Command::Reinstall {
            id,
            pack_version,
            java_type,
        } => {
            install::reinstall_instance(
                &paths,
                &id,
                &pack_version,
                java_type.as_deref(),
                progress_emit(),
            )
            .await?;
            println!("reinstalled {id} @ {pack_version}");
        }
        Command::PackVersions => {
            let versions = pack_files::fetch_gtnh_versions().await?;
            let mut keys: Vec<&String> = versions.keys().collect();
            keys.sort();
            print_json(&serde_json::json!({ "count": keys.len(), "versions": keys }));
        }
        Command::RefreshSizes { ids } => {
            let sizes =
                install::refresh_sizes(&paths, if ids.is_empty() { None } else { Some(&ids) })
                    .await?;
            print_json(&serde_json::to_value(&sizes)?);
        }
        Command::CheckLauncherUpdate { current_version } => {
            use industrialis_backend::release_updater::{check_for_update, Platform, UpdateCheck};
            let state = match check_for_update(&current_version, Platform::current()).await? {
                UpdateCheck::Current => {
                    serde_json::json!({"status": "up-to-date", "current_version": current_version})
                }
                UpdateCheck::Available(update) => {
                    serde_json::json!({"status": "available", "current_version": current_version,
                    "version": update.version, "release_url": update.release_url, "windows_installer": update.has_windows_installer()})
                }
            };
            print_json(&state);
        }
    }
    Ok(())
}

fn progress_emit() -> pack_files::EmitProgress {
    Arc::new(|progress| {
        let pct = (progress.pct * 100.0) as u32;
        match &progress.log_line {
            Some(line) => println!("[{} {pct}%] {line}", progress.stage),
            None => println!("[{} {pct}%]", progress.stage),
        }
    })
}

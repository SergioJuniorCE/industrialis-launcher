//! Opt-in real GTNH acceptance. Never run by `cargo test` or ordinary CI.
//! All writes stay in a marked expendable profile; no default profile is used.
use anyhow::{ensure, Context};
use industrialis_backend::{
    custom_mods, install, instance_ops, launch_config, minecraft_files, pack_files,
    paths::LauncherPaths, profile_owner, settings,
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

const MARKER: &str = "industrialis-expendable-live-acceptance-v1";
const ID: &str = "LiveModern";
const CONFIG: &str = "config/launcher-acceptance.cfg";
const CONFIG_BYTES: &str =
    "# Live pack preservation fixture\r\ngeneral {\r\n    B:enabled=true\r\n}\r\n";
const DATA: &[(&str, &[u8])] = &[
    (
        ".minecraft/saves/launcher-acceptance/level.dat",
        b"expendable save bytes\0\xff",
    ),
    (
        ".minecraft/journeymap/data/sp/launcher-acceptance/test.bin",
        b"expendable map bytes",
    ),
    (".minecraft/options.txt", b"music:0.0\nsound:0.0\n"),
];

fn main() -> anyhow::Result<()> {
    if industrialis_backend::game_broker::run_if_requested() {
        return Ok(());
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 3,
        "Usage: live_pack_acceptance <expendable-profile> <target-version> <previous-version>"
    );
    let profile = PathBuf::from(&args[0]);
    ensure!(
        profile.is_absolute(),
        "Use an absolute expendable profile path"
    );
    let marker = profile.join(".gpui-live-acceptance");
    if profile.exists() {
        ensure!(
            std::fs::read_to_string(&marker).ok().as_deref() == Some(MARKER),
            "Existing directory is not a marked expendable acceptance profile"
        );
    } else {
        std::fs::create_dir_all(&profile)?;
        std::fs::write(&marker, MARKER)?;
    }
    ensure!(
        !std::fs::symlink_metadata(&profile)?
            .file_type()
            .is_symlink(),
        "Profile must not be a symlink"
    );
    let paths = LauncherPaths::new(profile);
    let _owner = match profile_owner::acquire(&paths, profile_owner::OwnerKind::Operation)? {
        profile_owner::Acquisition::Primary(owner) => owner,
        _ => anyhow::bail!("Acceptance profile is already in use"),
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(run(&paths, &args[1], &args[2]))
}

fn progress() -> pack_files::EmitProgress {
    let last = Mutex::new(String::new());
    Arc::new(move |event| {
        let mut last = last.lock().unwrap();
        if *last != event.stage || event.log_line.is_some() {
            println!(
                "[{}] {}",
                event.stage,
                event.log_line.as_deref().unwrap_or_default()
            );
            *last = event.stage;
        }
    })
}

async fn assert_preserved(paths: &LauncherPaths, id: &str, kept_mod: bool) -> anyhow::Result<()> {
    let instance = paths.instance_dir(id)?;
    for (relative, expected) in DATA {
        ensure!(
            tokio::fs::read(instance.join(relative)).await? == *expected,
            "Player data changed: {id}/{relative}"
        );
    }
    for root in [".minecraft", "persistent-minecraft"] {
        ensure!(
            tokio::fs::read(instance.join(root).join(CONFIG)).await? == CONFIG_BYTES.as_bytes(),
            "Config overlay changed: {id}/{root}/{CONFIG}"
        );
        let present =
            tokio::fs::try_exists(instance.join(root).join("mods/launcher-acceptance-1.0.jar")).await?;
        ensure!(
            present == kept_mod,
            "Custom mod preservation mismatch: {id}/{root}"
        );
    }
    let saved = settings::load_instance_settings(paths, id).await?;
    ensure!(
        saved.extra_fields.get("gpui_acceptance_extension")
            == Some(&serde_json::json!({"retained":true})),
        "Unknown settings were lost"
    );
    Ok(())
}

async fn run(paths: &LauncherPaths, target: &str, previous: &str) -> anyhow::Result<()> {
    let mut steps = Vec::new();
    for (id, java_type) in [(ID, "java17+"), ("LiveLegacy", "java8")] {
        if !tokio::fs::try_exists(paths.settings_path(id)?).await? {
            install::download_install(
                paths,
                id,
                target,
                java_type,
                Some(id),
                Some("Launcher acceptance"),
                progress(),
            )
            .await?;
        }
        let saved = settings::load_instance_settings(paths, id).await?;
        ensure!(
            saved.pack_version == target && saved.pack_java_type == java_type,
            "Resume profile contains a different pack for {id}"
        );
        let emit: launch_config::EmitLaunchLog =
            Arc::new(|_, stream, line| println!("[{stream}] {line}"));
        let config = launch_config::build_launch_config(&paths.instance_dir(id)?, id, &emit)
            .await
            .with_context(|| format!("Resolve real {java_type} launch dependencies"))?;
        ensure!(
            !config.main_class.is_empty() && !config.libraries.is_empty(),
            "Incomplete launch config"
        );
        steps.push(serde_json::json!({"operation":"install-and-resolve","id":id,"version":target,
            "java_type":java_type,"main_class":config.main_class,"libraries":config.libraries.len()}));
    }
    let instance = paths.instance_dir(ID)?;
    for (relative, bytes) in DATA {
        let file = instance.join(relative);
        tokio::fs::create_dir_all(file.parent().unwrap()).await?;
        tokio::fs::write(file, bytes).await?;
    }
    minecraft_files::write_file(paths, ID, CONFIG, CONFIG_BYTES.into(), true, None).await?;
    // A valid empty ZIP is used as an expendable custom mod; remove before gameplay.
    let source = paths.base().join("launcher-acceptance-1.0.jar");
    tokio::fs::write(&source, b"PK\x05\x06\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0").await?;
    let custom = custom_mods::add(paths, ID, &source).await?;
    let mut saved = settings::load_instance_settings(paths, ID).await?;
    saved.extra_fields.insert(
        "gpui_acceptance_extension".into(),
        serde_json::json!({"retained":true}),
    );
    settings::save_instance_settings(paths, ID, &saved).await?;
    assert_preserved(paths, ID, true).await?;

    let copy = format!("LiveCopy{}", uuid::Uuid::new_v4().simple());
    instance_ops::copy_instance_with_progress(paths, ID, &copy, "Live acceptance copy", progress())
        .await?;
    assert_preserved(paths, &copy, true).await?;
    steps.push(serde_json::json!({"operation":"copy","id":copy,"preserved":true}));
    install::reinstall_instance(paths, ID, target, None, progress()).await?;
    assert_preserved(paths, ID, true).await?;
    steps.push(serde_json::json!({"operation":"reinstall","version":target,"preserved":true}));

    let preview = install::preview_update(paths, ID, previous, None, progress()).await?;
    ensure!(
        preview
            .custom_mods
            .iter()
            .any(|entry| entry.entry.identity == custom.identity),
        "Preview omitted custom mod"
    );
    install::update_instance(
        paths,
        ID,
        previous,
        None,
        &[custom.identity.clone()],
        progress(),
    )
    .await?;
    assert_preserved(paths, ID, true).await?;
    ensure!(
        settings::load_instance_settings(paths, ID)
            .await?
            .pack_version
            == previous,
        "Version did not change"
    );
    steps.push(serde_json::json!({"operation":"replace-with-previous","version":previous,"preserved":true}));
    install::update_instance(paths, ID, target, None, &[], progress()).await?;
    assert_preserved(paths, ID, false).await?;
    ensure!(
        settings::load_instance_settings(paths, ID)
            .await?
            .pack_version
            == target,
        "Update version did not persist"
    );
    steps.push(serde_json::json!({"operation":"update","version":target,"preserved":true,"unchecked_mod_removed":true}));
    let report = serde_json::json!({"platform":std::env::consts::OS,"arch":std::env::consts::ARCH,
        "profile":paths.base(),"steps":steps,"gameplay":"not tested","providers":"not tested"});
    tokio::fs::write(
        paths.base().join("pack-acceptance.json"),
        serde_json::to_vec_pretty(&report)?,
    )
    .await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

//! Instance delete + copy. Ports `deleteInstance`/`copyInstance` from
//! `electron/backend/instance-lifecycle.ts`.
//!
//! File deletion reports progress and admits explicit cancellation before
//! final metadata cleanup. Copy reuses the shared `copy_tree`.

use crate::fs_utils::{copy_tree_with_progress, dir_size, remove_if_exists};
use crate::groups::{get_instance_group, remove_instance_from_groups, set_instance_group};
use crate::icons::{install_default_instance_icon, resolve_icon_path};
use crate::instances::known_instance_ids;
use crate::paths::{validate_instance_id, LauncherPaths};
use crate::settings::{load_instance_settings, save_instance_settings};

pub const DELETE_CANCELLED: &str="Deletion cancelled; already removed files cannot be restored.";
/// Delete one file at a time, keeping discovery/settings until final cleanup.
/// Cancellation stops further deletion, never claims to restore removed data.
pub async fn delete_instance(paths:&LauncherPaths,raw_id:&str)->anyhow::Result<()> {
    delete_instance_with_progress(paths,raw_id,std::sync::Arc::new(|_|{}),std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false))).await
}
pub async fn delete_instance_with_progress(paths:&LauncherPaths,raw_id:&str,emit:crate::pack_files::EmitProgress,cancel:std::sync::Arc<std::sync::atomic::AtomicBool>)->anyhow::Result<()> {
    let id=validate_instance_id(raw_id)?;
    let lease=crate::instance_guard::reserve_instances(paths,&[&id])?;
    if crate::launch::is_already_running(paths,&id).await?{anyhow::bail!("cannot delete while instance is running");}
    crate::minecraft_files::stop_instance_watches(paths,&id).await?;
    let paths=paths.clone();
    // Dropping a caller future must not release ownership while an OS delete
    // is still completing. This task owns its reservation through finalization.
    tokio::spawn(async move {
        let _lease=lease;
        let instance=paths.instance_dir(&id)?;
        if !instance.exists(){return Ok(());}
        anyhow::ensure!(!std::fs::symlink_metadata(&instance)?.file_type().is_symlink(),"Cannot delete a linked instance directory");
        let root=instance.clone();
        let files=tokio::task::spawn_blocking(move||{
            let mut files=Vec::new();
            for entry in walkdir::WalkDir::new(&root).follow_links(false){
                let entry=entry?;
                anyhow::ensure!(!entry.file_type().is_symlink(),"Linked paths must be removed explicitly before deleting an instance");
                if entry.file_type().is_file() && entry.path()!=root.join("mmc-pack.json") && entry.path()!=root.join("instance.json") {files.push(entry.into_path());}
            }
            files.sort();Ok::<_,anyhow::Error>(files)
        }).await??;
        let ctx=crate::pack_files::ProgressCtx{emit,operation:"delete".into(),id:Some(id.clone())};
        ctx.progress("deleting",0.0,|p|p.log_line=Some("Deleting files; cancellation cannot restore files already removed.".into()));
        let total=files.len().max(1);
        for (index,file) in files.iter().enumerate(){
            if cancel.load(std::sync::atomic::Ordering::Acquire){anyhow::bail!(DELETE_CANCELLED);}
            tokio::fs::remove_file(file).await?;
            ctx.progress("deleting",(index+1)as f64/total as f64,|p|{if (index+1)%100==0 || index+1==files.len(){p.log_line=Some(format!("Deleted {} of {} files",index+1,files.len()));}});
        }
        if cancel.load(std::sync::atomic::Ordering::Acquire){anyhow::bail!(DELETE_CANCELLED);}
        // Commit boundary: finish metadata/group cleanup even if Cancel arrives.
        remove_if_exists(&instance).await?;
        let known=known_instance_ids(&paths).await?;
        remove_instance_from_groups(&paths,&id,&known).await?;
        ctx.progress("done",1.0,|_|{});Ok::<_,anyhow::Error>(())
    }).await?
}

/// Mirrors `copyInstance`: duplicate the tree, rename settings, re-resolve the
/// icon, refresh size, and inherit the source group.
pub async fn copy_instance(
    paths: &LauncherPaths,
    source_raw: &str,
    new_raw: &str,
    new_name_raw: &str,
) -> Result<(), anyhow::Error> {
    copy_instance_with_progress(
        paths,
        source_raw,
        new_raw,
        new_name_raw,
        std::sync::Arc::new(|_| {}),
    )
    .await
}

pub async fn copy_instance_with_progress(
    paths: &LauncherPaths,
    source_raw: &str,
    new_raw: &str,
    new_name_raw: &str,
    emit: crate::pack_files::EmitProgress,
) -> Result<(), anyhow::Error> {
    let source_id = validate_instance_id(source_raw)?;
    let new_id = validate_instance_id(new_raw)?;
    let new_name = new_name_raw.trim();
    if new_name.is_empty() {
        anyhow::bail!("instance name cannot be empty");
    }
    if source_id == new_id {
        anyhow::bail!("new instance id must differ from the source");
    }
    let _lease = crate::instance_guard::reserve_instances(paths, &[&source_id, &new_id])?;
    if crate::launch::is_already_running(paths, &source_id).await? {
        anyhow::bail!("cannot copy while instance is running");
    }
    crate::minecraft_files::stop_instance_watches(paths, &source_id).await?;
    let known = known_instance_ids(paths).await?;
    if !known.contains(&source_id) {
        anyhow::bail!("source instance not found");
    }
    if known.contains(&new_id) {
        anyhow::bail!("an instance with that id already exists");
    }
    let source = paths.instance_dir(&source_id)?;
    let destination = paths.instance_dir(&new_id)?;
    tokio::fs::create_dir_all(paths.instances_dir()).await?;
    // Claim any destination, including directories without mmc-pack.json.
    tokio::fs::create_dir(&destination)
        .await
        .map_err(|e| anyhow::anyhow!("Cannot create copy destination: {e}"))?;
    let ctx = crate::pack_files::ProgressCtx {
        emit,
        operation: "copy".into(),
        id: Some(new_id.clone()),
    };
    let progress = ctx.clone();
    let result = async {
        copy_tree_with_progress(&source, &destination, move |copied, total| {
            progress.progress(
                "copying",
                if total == 0 {
                    0.0
                } else {
                    copied as f64 / total as f64
                },
                |p| {
                    p.downloaded_mb = Some(copied as f64 / 1_048_576.0);
                    p.total_mb = Some(total as f64 / 1_048_576.0);
                },
            );
        })
        .await?;
        let mut settings = load_instance_settings(paths, &new_id).await?;
        settings.name = new_name.to_string();
        if resolve_icon_path(&destination, settings.custom_icon.as_deref())
            .await
            .is_none()
        {
            settings.custom_icon = Some(install_default_instance_icon(&destination).await?);
        }
        settings.cached_size_bytes = dir_size(&destination).await;
        save_instance_settings(paths, &new_id, &settings).await?;
        let group = get_instance_group(paths, &source_id, &known)
            .await
            .unwrap_or_default();
        if !group.is_empty() {
            let mut grown = known.clone();
            grown.insert(new_id.clone());
            set_instance_group(paths, &new_id, &group, &grown).await?;
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;
    if result.is_err() {
        remove_if_exists(&destination).await?;
    }
    result?;
    ctx.progress("done", 1.0, |_| {});
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::load_instances;

    fn temp_paths(name: &str) -> LauncherPaths {
        let dir = std::env::temp_dir().join(format!(
            "industrialis-ops-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        LauncherPaths::new(dir)
    }

    async fn seed(paths: &LauncherPaths, id: &str) {
        let dir = paths.instance_dir(id).unwrap();
        tokio::fs::create_dir_all(dir.join(".minecraft"))
            .await
            .unwrap();
        tokio::fs::write(dir.join("mmc-pack.json"), "{}")
            .await
            .unwrap();
        let mut settings = crate::settings::load_instance_settings(paths, id)
            .await
            .unwrap();
        settings.name = "Original".to_string();
        crate::settings::save_instance_settings(paths, id, &settings)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn cancellation_keeps_discovery_groups_and_undeleted_files_then_allows_retry() {
        let paths=temp_paths("delete-cancellation");
        seed(&paths,"pack").await;
        let dir=paths.instance_dir("pack").unwrap();
        for name in ["first","second","third"] {std::fs::write(dir.join(".minecraft").join(name),name).unwrap();}
        let known=known_instance_ids(&paths).await.unwrap();
        set_instance_group(&paths,"pack","Main",&known).await.unwrap();
        let settings=std::fs::read(paths.settings_path("pack").unwrap()).unwrap();
        let cancel=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let signal=cancel.clone();
        let emit=std::sync::Arc::new(move|p:crate::pack_files::PackProgress|{if p.pct>0.0 {signal.store(true,std::sync::atomic::Ordering::Release);}});
        let error=delete_instance_with_progress(&paths,"pack",emit,cancel).await.unwrap_err();
        assert_eq!(error.to_string(),DELETE_CANCELLED);
        assert_eq!(std::fs::read(paths.settings_path("pack").unwrap()).unwrap(),settings);
        assert!(dir.join("mmc-pack.json").exists());
        assert!(!dir.join(".minecraft/first").exists());
        assert!(dir.join(".minecraft/second").exists());
        let known=known_instance_ids(&paths).await.unwrap();assert!(known.contains("pack"));
        assert_eq!(get_instance_group(&paths,"pack",&known).await.unwrap(),"Main");
        delete_instance(&paths,"pack").await.unwrap();
        assert!(!dir.exists());
        std::fs::remove_dir_all(paths.base()).unwrap();
    }

    #[tokio::test]
    async fn copies_then_deletes() {
        let paths = temp_paths("cycle");
        seed(&paths, "orig").await;
        copy_instance(&paths, "orig", "copy", "Copy").await.unwrap();
        let list = load_instances(&paths).await.unwrap();
        assert_eq!(list.len(), 2);
        let copy = list.iter().find(|i| i.id == "copy").unwrap();
        assert_eq!(copy.settings.name, "Copy");
        assert!(copy.settings.custom_icon.is_some());

        delete_instance(&paths, "copy").await.unwrap();
        let list = load_instances(&paths).await.unwrap();
        assert_eq!(list.len(), 1);

        assert!(copy_instance(&paths, "orig", "orig", "Same").await.is_err());
        assert!(copy_instance(&paths, "missing", "new", "New")
            .await
            .is_err());
        assert!(copy_instance(&paths, "orig", "copy2", "").await.is_err());
    }

    #[tokio::test]
    async fn copy_rejects_busy_source_destination_and_opaque_collisions() {
        let paths = temp_paths("copy-guards");
        seed(&paths, "orig").await;
        for reserved in ["orig", "copy"] {
            let lease = crate::instance_guard::reserve_instances(&paths, &[reserved]).unwrap();
            assert!(copy_instance(&paths, "orig", "copy", "Copy").await.is_err());
            assert!(!paths.instance_dir("copy").unwrap().exists());
            assert!(delete_instance(&paths, reserved).await.is_err());
            drop(lease);
        }
        let destination = paths.instance_dir("copy").unwrap();
        tokio::fs::create_dir_all(&destination).await.unwrap();
        tokio::fs::write(destination.join("unrelated.txt"), "keep")
            .await
            .unwrap();
        assert!(copy_instance(&paths, "orig", "copy", "Copy").await.is_err());
        assert_eq!(
            tokio::fs::read_to_string(destination.join("unrelated.txt"))
                .await
                .unwrap(),
            "keep"
        );
        assert!(!destination.join("mmc-pack.json").exists());
    }

    #[tokio::test]
    async fn saved_order_survives_copy_move_and_delete_without_changing_other_groups() {
        use crate::groups::{change_groups, groups_state, GroupChange};
        let paths = LauncherPaths::new(std::env::temp_dir().join(format!("industrialis-order-lifecycle-{}", uuid::Uuid::new_v4())));
        for id in ["first", "second", "third", "other"] { seed(&paths, id).await; }
        let document = serde_json::json!({
            "groups": {
                "Main": { "hidden": false, "instances": ["first","second","third"], "extra": "retained" },
                "Other": { "hidden": true, "instances": ["other"] }
            },
            "ungrouped": { "hidden": false, "instances": [], "name": "Loose" },
            "futureExtension": [1,2]
        });
        std::fs::write(paths.groups_path(), document.to_string()).unwrap();
        let source = paths.instance_dir("second").unwrap();
        std::fs::write(source.join(".minecraft/player-data"), b"owned player data").unwrap();
        change_groups(&paths, GroupChange::Order { group: "Main".into(), order: vec!["third".into(), "first".into(), "second".into()] }).await.unwrap();
        assert_eq!(groups_state(&paths).await.unwrap().instance_order["Main"], ["third","first","second"]);
        copy_instance(&paths, "second", "copy", "Copy").await.unwrap();
        assert_eq!(groups_state(&paths).await.unwrap().instance_order["Main"], ["third","first","second","copy"]);
        change_groups(&paths, GroupChange::Place { id: "first".into(), group: "Other".into(), before: Some("other".into()) }).await.unwrap();
        delete_instance(&paths, "second").await.unwrap();
        let reloaded = groups_state(&paths).await.unwrap();
        assert_eq!(reloaded.instance_order["Main"], ["third","copy"]);
        assert_eq!(reloaded.instance_order["Other"], ["first","other"]);
        assert!(reloaded.collapsed.contains("Other"));
        assert_eq!(reloaded.ungrouped_name, "Loose");
        assert_eq!(std::fs::read(paths.instance_dir("copy").unwrap().join(".minecraft/player-data")).unwrap(), b"owned player data");
        let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(paths.groups_path()).unwrap()).unwrap();
        assert_eq!(saved["groups"]["Main"]["extra"], "retained");
        assert_eq!(saved["futureExtension"], document["futureExtension"]);
        assert!(!paths.instance_dir("second").unwrap().exists());
        for id in ["first", "third", "other"] { assert!(paths.instance_dir(id).unwrap().join("mmc-pack.json").exists()); }
        std::fs::remove_dir_all(paths.base()).unwrap();
    }

    #[tokio::test]
    async fn copy_reports_bytes_preserves_source_and_inherits_settings() {
        let paths = temp_paths("copy-progress");
        seed(&paths, "orig").await;
        let source = paths.instance_dir("orig").unwrap();
        tokio::fs::create_dir_all(source.join(".minecraft/saves/world"))
            .await
            .unwrap();
        tokio::fs::write(
            source.join(".minecraft/saves/world/level.dat"),
            "world data",
        )
        .await
        .unwrap();
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = events.clone();
        copy_instance_with_progress(
            &paths,
            "orig",
            "copy",
            "Backup",
            std::sync::Arc::new(move |p| sink.lock().unwrap().push(p)),
        )
        .await
        .unwrap();
        let destination = paths.instance_dir("copy").unwrap();
        assert_eq!(
            tokio::fs::read(destination.join(".minecraft/saves/world/level.dat"))
                .await
                .unwrap(),
            b"world data"
        );
        assert_eq!(
            load_instance_settings(&paths, "orig").await.unwrap().name,
            "Original"
        );
        assert_eq!(
            load_instance_settings(&paths, "copy").await.unwrap().name,
            "Backup"
        );
        let events = events.lock().unwrap();
        let copied = events
            .iter()
            .filter(|p| p.stage == "copying")
            .last()
            .unwrap();
        assert_eq!(copied.pct, 1.0);
        assert_eq!(copied.downloaded_mb, copied.total_mb);
        assert!(copied.total_mb.unwrap() > 0.0);
        assert_eq!(events.last().unwrap().stage, "done");
    }
}

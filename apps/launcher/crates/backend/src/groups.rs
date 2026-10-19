//! Port of the read/write subset of `electron/backend/groups.ts` needed by
//! install and listing: group assignment lookup + `setInstanceGroup`.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::fs_utils::write_json;
use crate::paths::LauncherPaths;

const DEFAULT_UNGROUPED_NAME: &str = "Ungrouped";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupsState {
    pub instance_groups: HashMap<String, String>,
    pub instance_order: HashMap<String, Vec<String>>,
    pub collapsed: HashSet<String>,
    pub ungrouped_name: String,
}
impl From<GroupData> for GroupsState {
    fn from(data: GroupData) -> Self {
        Self {
            instance_groups: data.instance_index,
            instance_order: data.group_order,
            collapsed: data.collapsed,
            ungrouped_name: data.ungrouped_name,
        }
    }
}
#[derive(Debug, Clone)]
pub enum GroupChange {
    Assign {
        id: String,
        group: String,
    },
    Move {
        id: String,
        up: bool,
    },
    Order {
        group: String,
        order: Vec<String>,
    },
    Place {
        id: String,
        group: String,
        before: Option<String>,
    },
    Rename {
        old: String,
        name: String,
    },
    Delete(String),
    Create(String),
    Collapse {
        group: String,
        collapsed: bool,
    },
}
pub async fn groups_state(paths: &LauncherPaths) -> anyhow::Result<GroupsState> {
    let known = crate::instances::known_instance_ids(paths).await?;
    Ok(load_data(paths, &known).await?.into())
}
/// Each edit reads the authoritative file under the profile transaction lock.
/// No workspace snapshots, metadata import, or changes to unrelated documents.
pub async fn change_groups(
    paths: &LauncherPaths,
    change: GroupChange,
) -> anyhow::Result<GroupsState> {
    let lease = crate::lifecycle::admit(paths)?;
    let paths = paths.clone();
    crate::profile_store::mutate(async move {
        let _lease = lease;
        let known = crate::instances::known_instance_ids(&paths).await?;
        let mut data = load_data(&paths, &known).await?;
        match change {
            GroupChange::Assign { id, group } => {
                anyhow::ensure!(known.contains(&id), "Instance not found");
                let group = group.trim().to_string();
                validate_name(&group, true)?;
                let old = data.instance_index.remove(&id).unwrap_or_default();
                remove_from_order(&mut data, &id, &old);
                if !group.is_empty() {
                    data.instance_index.insert(id.clone(), group.clone());
                }
                append_to_order(&mut data, &id, &group);
            }
            GroupChange::Move { id, up } => {
                anyhow::ensure!(known.contains(&id), "Instance not found");
                let group = data.instance_index.get(&id).cloned().unwrap_or_default();
                let order = data
                    .group_order
                    .get_mut(&group)
                    .ok_or_else(|| anyhow::anyhow!("Group order not found"))?;
                let index = order
                    .iter()
                    .position(|v| v == &id)
                    .ok_or_else(|| anyhow::anyhow!("Instance order not found"))?;
                if up && index > 0 {
                    order.swap(index, index - 1);
                } else if !up && index + 1 < order.len() {
                    order.swap(index, index + 1);
                }
            }
            GroupChange::Order { group, order } => {
                if !known.iter().any(|id| {
                    data.instance_index
                        .get(id)
                        .map(String::as_str)
                        .unwrap_or("")
                        == group
                }) {
                    return Ok(data.into());
                }
                data.group_order.insert(group, order);
                reconcile(&mut data, &known);
            }
            GroupChange::Place { id, group, before } => {
                anyhow::ensure!(known.contains(&id), "Instance not found");
                anyhow::ensure!(
                    group.is_empty() || data.group_order.contains_key(&group),
                    "Group not found"
                );
                if before.as_ref() == Some(&id) {
                    return Ok(data.into());
                }
                if let Some(target) = &before {
                    anyhow::ensure!(
                        known.contains(target)
                            && data
                                .instance_index
                                .get(target)
                                .map(String::as_str)
                                .unwrap_or("")
                                == group,
                        "Target instance moved; try again"
                    );
                }
                let old = data.instance_index.remove(&id).unwrap_or_default();
                remove_from_order(&mut data, &id, &old);
                if !group.is_empty() {
                    data.instance_index.insert(id.clone(), group.clone());
                }
                let order = data.group_order.entry(group).or_default();
                let position = before
                    .as_ref()
                    .and_then(|target| order.iter().position(|item| item == target))
                    .unwrap_or(order.len());
                order.insert(position, id);
            }
            GroupChange::Rename { old, name } => {
                let name = name.trim().to_string();
                validate_name(&name, false)?;
                let old = old.trim().to_string();
                if old.is_empty() {
                    ensure_unique(&data, &name, "", true)?;
                    data.ungrouped_name = name;
                } else if old.to_lowercase() != name.to_lowercase() {
                    anyhow::ensure!(data.group_order.contains_key(&old), "Group not found");
                    ensure_unique(&data, &name, &old, false)?;
                    for group in data.instance_index.values_mut() {
                        if *group == old {
                            *group = name.clone();
                        }
                    }
                    let order = data.group_order.remove(&old).unwrap_or_default();
                    data.group_order.insert(name.clone(), order);
                    if data.collapsed.remove(&old) {
                        data.collapsed.insert(name.clone());
                    }
                    if let Some(groups) = data
                        .document
                        .get_mut("groups")
                        .and_then(|v| v.as_object_mut())
                    {
                        if let Some(entry) = groups.remove(&old) {
                            groups.insert(name, entry);
                        }
                    }
                }
            }
            GroupChange::Delete(group) => {
                anyhow::ensure!(!group.is_empty(), "Cannot delete the ungrouped section");
                let order = data.group_order.remove(&group).unwrap_or_default();
                for id in order {
                    if data.instance_index.get(&id) == Some(&group) {
                        data.instance_index.remove(&id);
                        append_to_order(&mut data, &id, "");
                    }
                }
                data.collapsed.remove(&group);
                if let Some(groups) = data
                    .document
                    .get_mut("groups")
                    .and_then(|v| v.as_object_mut())
                {
                    groups.remove(&group);
                }
            }
            GroupChange::Create(name) => {
                let name = name.trim().to_string();
                validate_name(&name, false)?;
                ensure_unique(&data, &name, "", false)?;
                data.group_order.insert(name, Vec::new());
            }
            GroupChange::Collapse { group, collapsed } => {
                anyhow::ensure!(
                    group.is_empty() || data.group_order.contains_key(&group),
                    "Group not found"
                );
                if collapsed {
                    data.collapsed.insert(group);
                } else {
                    data.collapsed.remove(&group);
                }
            }
        }
        save_data(&paths, &data, &known).await?;
        Ok(data.into())
    })
    .await
}
fn validate_name(name: &str, allow_empty: bool) -> anyhow::Result<()> {
    anyhow::ensure!(
        allow_empty || !name.is_empty(),
        "Group name cannot be empty"
    );
    anyhow::ensure!(name.chars().count() <= 128, "Group name is too long");
    Ok(())
}
fn ensure_unique(
    data: &GroupData,
    name: &str,
    except: &str,
    renaming_ungrouped: bool,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        !data
            .group_order
            .keys()
            .any(|group| group != except && group.to_lowercase() == name.to_lowercase())
            && (renaming_ungrouped || data.ungrouped_name.to_lowercase() != name.to_lowercase()),
        "A group with that name already exists"
    );
    Ok(())
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct GroupEntry {
    #[serde(default)]
    hidden: bool,
    #[serde(default)]
    instances: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct GroupFile {
    #[serde(rename = "formatVersion")]
    #[allow(dead_code)]
    format_version: Option<u32>,
    #[serde(default)]
    groups: std::collections::BTreeMap<String, GroupEntry>,
    #[serde(default)]
    ungrouped: UngroupedEntry,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct UngroupedEntry {
    #[serde(default)]
    hidden: bool,
    #[serde(default)]
    instances: Vec<String>,
    #[serde(default)]
    name: Option<String>,
}

struct GroupData {
    document: serde_json::Value,
    instance_index: HashMap<String, String>,
    group_order: HashMap<String, Vec<String>>,
    collapsed: HashSet<String>,
    ungrouped_name: String,
}

async fn load_data(
    paths: &LauncherPaths,
    known: &HashSet<String>,
) -> Result<GroupData, anyhow::Error> {
    let document = crate::profile_store::read_object(&paths.groups_path())
        .await?
        .unwrap_or_else(|| serde_json::json!({}));
    let file: GroupFile = serde_json::from_value(document.clone()).map_err(|e| {
        anyhow::anyhow!(
            "Invalid group data in {}: {e}. The original file has been kept.",
            paths.groups_path().display()
        )
    })?;
    let mut instance_index = HashMap::new();
    let mut group_order: HashMap<String, Vec<String>> = HashMap::new();
    let mut collapsed = HashSet::new();
    if file.ungrouped.hidden {
        collapsed.insert(String::new());
    }
    let ungrouped_name = file
        .ungrouped
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_UNGROUPED_NAME)
        .to_string();
    let mut ungrouped_order: Vec<String> = file
        .ungrouped
        .instances
        .into_iter()
        .filter(|id| known.contains(id))
        .collect();
    group_order.insert(String::new(), Vec::new());

    for (name, entry) in &file.groups {
        if entry.hidden {
            collapsed.insert(name.clone());
        }
        let ids: Vec<String> = entry
            .instances
            .iter()
            .filter(|id| known.contains(*id))
            .cloned()
            .collect();
        for id in &ids {
            instance_index.insert(id.clone(), name.clone());
        }
        group_order.insert(name.clone(), ids);
    }
    // Retain saved order, then append newly discovered members in the same
    // deterministic order as the library. HashSet iteration must never become
    // a persisted order on the first drag or group edit.
    let mut discovered: Vec<_> = known
        .iter()
        .filter(|id| !instance_index.contains_key(*id) && !ungrouped_order.contains(*id))
        .cloned()
        .collect();
    discovered.sort();
    ungrouped_order.extend(discovered);
    group_order.insert(String::new(), ungrouped_order);
    let mut data = GroupData {
        document,
        instance_index,
        group_order,
        collapsed,
        ungrouped_name,
    };
    reconcile(&mut data, known);
    Ok(data)
}

fn reconcile(data: &mut GroupData, known: &HashSet<String>) {
    let mut members: HashMap<String, HashSet<String>> = HashMap::new();
    for id in known {
        let group = data.instance_index.get(id).cloned().unwrap_or_default();
        members.entry(group).or_default().insert(id.clone());
    }
    for (group, member_set) in &members {
        let order = data.group_order.get(group).cloned().unwrap_or_default();
        let mut seen = HashSet::new();
        let mut next: Vec<String> = order
            .into_iter()
            .filter(|id| member_set.contains(id) && seen.insert(id.clone()))
            .collect();
        let mut rest: Vec<String> = member_set
            .iter()
            .filter(|id| !seen.contains(*id))
            .cloned()
            .collect();
        rest.sort();
        next.append(&mut rest);
        data.group_order.insert(group.clone(), next);
    }
    // Empty named groups carry user-authored metadata and must survive saves.
}

async fn save_data(
    paths: &LauncherPaths,
    data: &GroupData,
    known: &HashSet<String>,
) -> Result<(), anyhow::Error> {
    let mut groups: HashMap<String, GroupEntry> = HashMap::new();
    for (name, order) in &data.group_order {
        if name.is_empty() {
            continue;
        }
        let mut seen = HashSet::new();
        let ids: Vec<String> = order
            .iter()
            .filter(|id| {
                known.contains(*id)
                    && data.instance_index.get(*id) == Some(name)
                    && seen.insert((*id).clone())
            })
            .cloned()
            .collect();
        groups.insert(
            name.clone(),
            GroupEntry {
                hidden: data.collapsed.contains(name),
                instances: ids,
            },
        );
    }
    let mut seen = HashSet::new();
    let ungrouped: Vec<String> = data
        .group_order
        .get("")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|id| {
            known.contains(id) && !data.instance_index.contains_key(id) && seen.insert(id.clone())
        })
        .collect();
    let mut document = data.document.clone();
    let root = document.as_object_mut().unwrap();
    root.entry("formatVersion").or_insert(serde_json::json!(1));
    let stored_groups = root
        .entry("groups")
        .or_insert(serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Invalid groups document"))?;
    for (name, entry) in groups {
        let object = stored_groups
            .entry(name)
            .or_insert(serde_json::json!({}))
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("Invalid group entry"))?;
        if object
            .get("hidden")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            != entry.hidden
        {
            object.insert("hidden".into(), serde_json::json!(entry.hidden));
        }
        let old: Vec<String> = object
            .get("instances")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        if old != entry.instances {
            object.insert("instances".into(), serde_json::json!(entry.instances));
        }
    }
    let ungrouped_object = root
        .entry("ungrouped")
        .or_insert(serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Invalid ungrouped entry"))?;
    if ungrouped_object
        .get("hidden")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        != data.collapsed.contains("")
    {
        ungrouped_object.insert(
            "hidden".into(),
            serde_json::json!(data.collapsed.contains("")),
        );
    }
    let old: Vec<String> = ungrouped_object
        .get("instances")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    if old != ungrouped {
        ungrouped_object.insert("instances".into(), serde_json::json!(ungrouped));
    }
    let old_name = ungrouped_object
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(DEFAULT_UNGROUPED_NAME);
    if old_name != data.ungrouped_name {
        ungrouped_object.insert("name".into(), serde_json::json!(data.ungrouped_name));
    }
    if crate::profile_store::read_object(&paths.groups_path())
        .await?
        .as_ref()
        == Some(&document)
    {
        return Ok(());
    }
    write_json(&paths.groups_path(), &document).await
}

/// Mirrors `getInstanceGroup`.
pub async fn get_instance_group(
    paths: &LauncherPaths,
    id: &str,
    known: &HashSet<String>,
) -> Result<String, anyhow::Error> {
    Ok(load_data(paths, known)
        .await?
        .instance_index
        .get(id)
        .cloned()
        .unwrap_or_default())
}

/// Mirrors `setInstanceGroup` (empty group clears the assignment).
pub async fn set_instance_group(
    paths: &LauncherPaths,
    id: &str,
    group: &str,
    known: &HashSet<String>,
) -> Result<(), anyhow::Error> {
    if !known.contains(id) {
        anyhow::bail!("instance not found");
    }
    let name = group.trim().to_string();
    if name.len() > 128 {
        anyhow::bail!("group name too long");
    }
    let paths = paths.clone();
    let id = id.to_string();
    let known = known.clone();
    crate::profile_store::mutate(async move {
        let mut data = load_data(&paths, &known).await?;
        let old = data.instance_index.get(&id).cloned().unwrap_or_default();
        if name.is_empty() {
            data.instance_index.remove(&id);
        } else {
            data.instance_index.insert(id.to_string(), name.to_string());
        }
        remove_from_order(&mut data, &id, &old);
        append_to_order(&mut data, &id, &name);
        save_data(&paths, &data, &known).await
    })
    .await
}

/// Mirrors `removeInstanceFromGroups`.
pub async fn remove_instance_from_groups(
    paths: &LauncherPaths,
    id: &str,
    known: &HashSet<String>,
) -> Result<(), anyhow::Error> {
    let paths = paths.clone();
    let id = id.to_string();
    let known = known.clone();
    crate::profile_store::mutate(async move {
        let mut data = load_data(&paths, &known).await?;
        let group = data.instance_index.get(&id).cloned().unwrap_or_default();
        data.instance_index.remove(&id);
        remove_from_order(&mut data, &id, &group);
        remove_from_order(&mut data, &id, "");
        save_data(&paths, &data, &known).await
    })
    .await
}

fn remove_from_order(data: &mut GroupData, id: &str, group: &str) {
    if let Some(order) = data.group_order.get(group).cloned() {
        data.group_order.insert(
            group.to_string(),
            order.into_iter().filter(|entry| entry != id).collect(),
        );
    }
}

fn append_to_order(data: &mut GroupData, id: &str, group: &str) {
    let mut order = data.group_order.get(group).cloned().unwrap_or_default();
    if !order.iter().any(|entry| entry == id) {
        order.push(id.to_string());
    }
    data.group_order.insert(group.to_string(), order);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn management_edits_the_original_file_and_preserves_order_and_extensions() {
        let paths = temp_paths("management");
        for id in ["a", "b", "c"] {
            let dir = paths.instance_dir(id).unwrap();
            tokio::fs::create_dir_all(&dir).await.unwrap();
            tokio::fs::write(dir.join("mmc-pack.json"), "{}")
                .await
                .unwrap();
        }
        write_json(
            &paths.groups_path(),
            &serde_json::json!({"formatVersion":1,"root_extra":true,
                "groups":{"Main":{"instances":["b","a"],"color":"keep","hidden":true}},
                "ungrouped":{"name":"Other","instances":["c"],"extra":17}
            }),
        )
        .await
        .unwrap();
        let state = change_groups(
            &paths,
            GroupChange::Rename {
                old: "Main".into(),
                name: "Games".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(state.instance_order["Games"], vec!["b", "a"]);
        assert!(state.collapsed.contains("Games"));
        let doc: serde_json::Value =
            serde_json::from_slice(&tokio::fs::read(paths.groups_path()).await.unwrap()).unwrap();
        assert_eq!(doc["groups"]["Games"]["color"], "keep");
        assert!(doc["groups"].get("Main").is_none());
        assert_eq!(doc["root_extra"], true);
        assert_eq!(doc["ungrouped"]["extra"], 17);
        assert!(change_groups(&paths, GroupChange::Create("gAmEs".into()))
            .await
            .is_err());
        assert!(change_groups(&paths, GroupChange::Create("Other".into()))
            .await
            .is_err());
        change_groups(&paths, GroupChange::Create("Empty".into()))
            .await
            .unwrap();
        change_groups(
            &paths,
            GroupChange::Assign {
                id: "c".into(),
                group: "Games".into(),
            },
        )
        .await
        .unwrap();
        let state = change_groups(
            &paths,
            GroupChange::Place {
                id: "c".into(),
                group: "Games".into(),
                before: Some("a".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(state.instance_order["Games"], vec!["b", "c", "a"]);
        let state = change_groups(
            &paths,
            GroupChange::Move {
                id: "b".into(),
                up: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(state.instance_order["Games"], vec!["c", "b", "a"]);
        let state = change_groups(
            &paths,
            GroupChange::Order {
                group: "Games".into(),
                order: vec!["a".into(), "a".into(), "invalid".into()],
            },
        )
        .await
        .unwrap();
        assert_eq!(state.instance_order["Games"], vec!["a", "b", "c"]);
        change_groups(
            &paths,
            GroupChange::Collapse {
                group: "Games".into(),
                collapsed: false,
            },
        )
        .await
        .unwrap();
        let state = change_groups(&paths, GroupChange::Delete("Games".into()))
            .await
            .unwrap();
        assert_eq!(state.instance_order[""], vec!["a", "b", "c"]);
        assert!(state.instance_groups.is_empty());
        assert!(state.instance_order.contains_key("Empty"));
        change_groups(
            &paths,
            GroupChange::Rename {
                old: "".into(),
                name: "Loose".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(groups_state(&paths).await.unwrap().ungrouped_name, "Loose");
        assert!(!paths.base().join("gpui-workspace.json").exists());
    }

    fn temp_paths(name: &str) -> LauncherPaths {
        let dir = std::env::temp_dir().join(format!(
            "industrialis-groups-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        LauncherPaths::new(dir)
    }

    fn known(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[tokio::test]
    async fn assigns_and_clears_groups() {
        let paths = temp_paths("assign");
        let known = known(&["a", "b"]);
        assert_eq!(get_instance_group(&paths, "a", &known).await.unwrap(), "");
        set_instance_group(&paths, "a", "favorites", &known)
            .await
            .unwrap();
        assert_eq!(
            get_instance_group(&paths, "a", &known).await.unwrap(),
            "favorites"
        );
        set_instance_group(&paths, "a", "", &known).await.unwrap();
        assert_eq!(get_instance_group(&paths, "a", &known).await.unwrap(), "");
        assert!(set_instance_group(&paths, "missing", "g", &known)
            .await
            .is_err());
    }
}

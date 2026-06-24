use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

const GROUP_FILE_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupFileEntry {
    pub hidden: bool,
    pub instances: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UngroupedEntry {
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GroupFile {
    #[serde(rename = "formatVersion")]
    format_version: u32,
    #[serde(default)]
    groups: HashMap<String, GroupFileEntry>,
    #[serde(default)]
    ungrouped: UngroupedEntry,
}

impl Default for UngroupedEntry {
    fn default() -> Self {
        Self { hidden: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceGroupsState {
    pub collapsed: HashMap<String, bool>,
    pub groups: Vec<String>,
}

struct GroupData {
    instance_index: HashMap<String, String>,
    collapsed: HashSet<String>,
}

pub fn groups_file_path(instances_dir: &PathBuf) -> PathBuf {
    instances_dir.join("instgroups.json")
}

fn load_group_data(instances_dir: &PathBuf, known_instances: &HashSet<String>) -> GroupData {
    let path = groups_file_path(instances_dir);
    let mut instance_index = HashMap::new();
    let mut collapsed = HashSet::new();

    let file: GroupFile = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(GroupFile {
            format_version: GROUP_FILE_FORMAT_VERSION,
            groups: HashMap::new(),
            ungrouped: UngroupedEntry::default(),
        });

    if file.ungrouped.hidden {
        collapsed.insert(String::new());
    }

    for (group_name, entry) in file.groups {
        if entry.hidden {
            collapsed.insert(group_name.clone());
        }
        for instance_id in entry.instances {
            if known_instances.contains(&instance_id) {
                instance_index.insert(instance_id, group_name.clone());
            }
        }
    }

    GroupData {
        instance_index,
        collapsed,
    }
}

fn save_group_data(
    instances_dir: &PathBuf,
    instance_index: &HashMap<String, String>,
    collapsed: &HashSet<String>,
    known_instances: &HashSet<String>,
) -> Result<(), String> {
    let mut groups: HashMap<String, GroupFileEntry> = HashMap::new();

    for (instance_id, group_name) in instance_index {
        if group_name.is_empty() || !known_instances.contains(instance_id) {
            continue;
        }
        groups
            .entry(group_name.clone())
            .or_insert_with(|| GroupFileEntry {
                hidden: collapsed.contains(group_name),
                instances: Vec::new(),
            })
            .instances
            .push(instance_id.clone());
    }

    for entry in groups.values_mut() {
        entry.instances.sort();
    }

    let mut group_names: Vec<_> = groups.keys().cloned().collect();
    group_names.sort();

    let mut ordered_groups = HashMap::new();
    for name in group_names {
        if let Some(mut entry) = groups.remove(&name) {
            entry.hidden = collapsed.contains(&name);
            ordered_groups.insert(name, entry);
        }
    }

    let file = GroupFile {
        format_version: GROUP_FILE_FORMAT_VERSION,
        groups: ordered_groups,
        ungrouped: UngroupedEntry {
            hidden: collapsed.contains(""),
        },
    };

    fs::create_dir_all(instances_dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    fs::write(groups_file_path(instances_dir), json).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn get_instance_group(
    instances_dir: &PathBuf,
    instance_id: &str,
    known_instances: &HashSet<String>,
) -> String {
    let data = load_group_data(instances_dir, known_instances);
    data.instance_index
        .get(instance_id)
        .cloned()
        .unwrap_or_default()
}

pub fn get_groups_state(
    instances_dir: &PathBuf,
    known_instances: &HashSet<String>,
) -> InstanceGroupsState {
    let data = load_group_data(instances_dir, known_instances);
    let mut group_counts: HashMap<String, usize> = HashMap::new();

    for instance_id in known_instances {
        let group = data
            .instance_index
            .get(instance_id)
            .cloned()
            .unwrap_or_default();
        *group_counts.entry(group).or_default() += 1;
    }

    let mut groups: Vec<String> = group_counts
        .keys()
        .filter(|g| !g.is_empty())
        .cloned()
        .collect();
    groups.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));

    let mut collapsed = HashMap::new();
    for group in &groups {
        collapsed.insert(group.clone(), data.collapsed.contains(group));
    }
    collapsed.insert(
        String::new(),
        data.collapsed.contains(""),
    );

    InstanceGroupsState { collapsed, groups }
}

pub fn set_instance_group(
    instances_dir: &PathBuf,
    instance_id: &str,
    group: &str,
    known_instances: &HashSet<String>,
) -> Result<(), String> {
    if !known_instances.contains(instance_id) {
        return Err("instance not found".into());
    }
    let group = group.trim();
    if group.len() > 128 {
        return Err("group name too long".into());
    }

    let mut data = load_group_data(instances_dir, known_instances);
    if group.is_empty() {
        data.instance_index.remove(instance_id);
    } else {
        data.instance_index.insert(instance_id.to_string(), group.to_string());
    }
    save_group_data(
        instances_dir,
        &data.instance_index,
        &data.collapsed,
        known_instances,
    )
}

pub fn rename_group(
    instances_dir: &PathBuf,
    old_name: &str,
    new_name: &str,
    known_instances: &HashSet<String>,
) -> Result<(), String> {
    let old_name = old_name.trim();
    let new_name = new_name.trim();
    if old_name.is_empty() {
        return Err("cannot rename ungrouped section".into());
    }
    if new_name.is_empty() {
        return Err("group name cannot be empty".into());
    }
    if new_name.len() > 128 {
        return Err("group name too long".into());
    }
    if old_name.eq_ignore_ascii_case(new_name) {
        return Ok(());
    }

    let mut data = load_group_data(instances_dir, known_instances);
    let has_old = data.instance_index.values().any(|g| g == old_name);
    if !has_old {
        return Err("group not found".into());
    }
    if data
        .instance_index
        .values()
        .any(|g| g.eq_ignore_ascii_case(new_name) && g != old_name)
    {
        return Err("a group with that name already exists".into());
    }

    for group in data.instance_index.values_mut() {
        if group == old_name {
            *group = new_name.to_string();
        }
    }

    let was_collapsed = data.collapsed.remove(old_name);
    if was_collapsed {
        data.collapsed.insert(new_name.to_string());
    }

    save_group_data(
        instances_dir,
        &data.instance_index,
        &data.collapsed,
        known_instances,
    )
}

pub fn delete_group(
    instances_dir: &PathBuf,
    name: &str,
    known_instances: &HashSet<String>,
) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("cannot delete ungrouped section".into());
    }

    let mut data = load_group_data(instances_dir, known_instances);
    data.instance_index.retain(|_, group| group != name);
    data.collapsed.remove(name);
    save_group_data(
        instances_dir,
        &data.instance_index,
        &data.collapsed,
        known_instances,
    )
}

pub fn set_group_collapsed(
    instances_dir: &PathBuf,
    group: &str,
    collapsed: bool,
    known_instances: &HashSet<String>,
) -> Result<(), String> {
    let mut data = load_group_data(instances_dir, known_instances);
    if collapsed {
        data.collapsed.insert(group.to_string());
    } else {
        data.collapsed.remove(group);
    }
    save_group_data(
        instances_dir,
        &data.instance_index,
        &data.collapsed,
        known_instances,
    )
}

pub fn remove_instance_from_groups(
    instances_dir: &PathBuf,
    instance_id: &str,
    known_instances: &HashSet<String>,
) -> Result<(), String> {
    let mut data = load_group_data(instances_dir, known_instances);
    data.instance_index.remove(instance_id);
    save_group_data(
        instances_dir,
        &data.instance_index,
        &data.collapsed,
        known_instances,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn temp_instances_dir() -> PathBuf {
        let dir = env::temp_dir().join(format!(
            "industrialis-groups-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn set_and_get_group() {
        let dir = temp_instances_dir();
        let known = HashSet::from(["inst-a".to_string(), "inst-b".to_string()]);

        set_instance_group(&dir, "inst-a", "Modpacks", &known).unwrap();
        assert_eq!(get_instance_group(&dir, "inst-a", &known), "Modpacks");
        assert_eq!(get_instance_group(&dir, "inst-b", &known), "");

        let state = get_groups_state(&dir, &known);
        assert_eq!(state.groups, vec!["Modpacks"]);
        assert_eq!(state.collapsed.get("Modpacks"), Some(&false));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn rename_group_moves_instances() {
        let dir = temp_instances_dir();
        let known = HashSet::from(["inst-a".to_string()]);

        set_instance_group(&dir, "inst-a", "Old", &known).unwrap();
        rename_group(&dir, "Old", "New", &known).unwrap();
        assert_eq!(get_instance_group(&dir, "inst-a", &known), "New");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn delete_group_ungroups_instances() {
        let dir = temp_instances_dir();
        let known = HashSet::from(["inst-a".to_string()]);

        set_instance_group(&dir, "inst-a", "Temp", &known).unwrap();
        delete_group(&dir, "Temp", &known).unwrap();
        assert_eq!(get_instance_group(&dir, "inst-a", &known), "");

        let _ = fs::remove_dir_all(dir);
    }
}
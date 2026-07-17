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
    #[serde(default)]
    pub instances: Vec<String>,
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
        Self {
            hidden: false,
            instances: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceGroupsState {
    pub collapsed: HashMap<String, bool>,
    pub groups: Vec<String>,
    pub instance_order: HashMap<String, Vec<String>>,
}

struct GroupData {
    instance_index: HashMap<String, String>,
    group_order: HashMap<String, Vec<String>>,
    collapsed: HashSet<String>,
}

pub fn groups_file_path(instances_dir: &PathBuf) -> PathBuf {
    instances_dir.join("instgroups.json")
}

fn load_group_data(instances_dir: &PathBuf, known_instances: &HashSet<String>) -> GroupData {
    let path = groups_file_path(instances_dir);
    let mut instance_index = HashMap::new();
    let mut group_order: HashMap<String, Vec<String>> = HashMap::new();
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

    let mut ungrouped_order: Vec<String> = file
        .ungrouped
        .instances
        .into_iter()
        .filter(|id| known_instances.contains(id))
        .collect();
    group_order.insert(String::new(), ungrouped_order.clone());

    for (group_name, entry) in file.groups {
        if entry.hidden {
            collapsed.insert(group_name.clone());
        }
        let ids: Vec<String> = entry
            .instances
            .into_iter()
            .filter(|id| known_instances.contains(id))
            .collect();
        for instance_id in &ids {
            instance_index.insert(instance_id.clone(), group_name.clone());
        }
        group_order.insert(group_name, ids);
    }

    for instance_id in known_instances {
        if instance_index.contains_key(instance_id) {
            continue;
        }
        ungrouped_order.push(instance_id.clone());
        group_order.insert(String::new(), ungrouped_order.clone());
    }

    let mut data = GroupData {
        instance_index,
        group_order,
        collapsed,
    };
    reconcile_group_orders(&mut data, known_instances);
    data
}

fn reconcile_group_orders(data: &mut GroupData, known_instances: &HashSet<String>) {
    let mut members: HashMap<String, HashSet<String>> = HashMap::new();
    for instance_id in known_instances {
        let group = data
            .instance_index
            .get(instance_id)
            .cloned()
            .unwrap_or_default();
        members
            .entry(group)
            .or_default()
            .insert(instance_id.clone());
    }

    for (group, member_set) in &members {
        let order = data.group_order.entry(group.clone()).or_default();
        order.retain(|id| member_set.contains(id));
        let mut deduped: Vec<String> = Vec::with_capacity(order.len());
        let mut seen: HashSet<String> = HashSet::new();
        for id in order.drain(..) {
            if seen.insert(id.clone()) {
                deduped.push(id);
            }
        }
        order.extend(deduped);
        let in_order: HashSet<_> = order.iter().cloned().collect();
        let mut newcomers: Vec<_> = member_set
            .iter()
            .filter(|id| !in_order.contains(*id))
            .cloned()
            .collect();
        newcomers.sort();
        order.extend(newcomers);
    }

    data.group_order
        .retain(|group, ids| members.contains_key(group) || !ids.is_empty());
}

fn save_group_data(
    instances_dir: &PathBuf,
    data: &GroupData,
    known_instances: &HashSet<String>,
) -> Result<(), String> {
    let mut groups: HashMap<String, GroupFileEntry> = HashMap::new();

    for (group_name, ids) in &data.group_order {
        if group_name.is_empty() {
            continue;
        }
        let mut seen: HashSet<String> = HashSet::new();
        let filtered: Vec<String> = ids
            .iter()
            .filter(|id| {
                known_instances.contains(*id)
                    && data.instance_index.get(*id).cloned().unwrap_or_default() == *group_name
            })
            .filter(|id| seen.insert((*id).clone()))
            .cloned()
            .collect();
        if filtered.is_empty() && !data.collapsed.contains(group_name) {
            continue;
        }
        groups.insert(
            group_name.clone(),
            GroupFileEntry {
                hidden: data.collapsed.contains(group_name),
                instances: filtered,
            },
        );
    }

    let mut ungrouped_seen: HashSet<String> = HashSet::new();
    let ungrouped_ids: Vec<String> = data
        .group_order
        .get("")
        .map(|ids| {
            ids.iter()
                .filter(|id| {
                    known_instances.contains(*id)
                        && data.instance_index.get(*id).is_none()
                })
                .filter(|id| ungrouped_seen.insert((*id).clone()))
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    let file = GroupFile {
        format_version: GROUP_FILE_FORMAT_VERSION,
        groups,
        ungrouped: UngroupedEntry {
            hidden: data.collapsed.contains(""),
            instances: ungrouped_ids,
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
    collapsed.insert(String::new(), data.collapsed.contains(""));

    InstanceGroupsState {
        collapsed,
        groups,
        instance_order: data.group_order,
    }
}

fn remove_from_group_order(data: &mut GroupData, instance_id: &str, group: &str) {
    if let Some(order) = data.group_order.get_mut(group) {
        order.retain(|id| id != instance_id);
    }
}

fn append_to_group_order(data: &mut GroupData, instance_id: &str, group: &str) {
    let order = data.group_order.entry(group.to_string()).or_default();
    if !order.iter().any(|id| id == instance_id) {
        order.push(instance_id.to_string());
    }
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
    let old_group = data
        .instance_index
        .get(instance_id)
        .cloned()
        .unwrap_or_default();

    if group.is_empty() {
        data.instance_index.remove(instance_id);
    } else {
        data.instance_index
            .insert(instance_id.to_string(), group.to_string());
    }

    remove_from_group_order(&mut data, instance_id, &old_group);
    let new_group = group.to_string();
    append_to_group_order(&mut data, instance_id, &new_group);

    save_group_data(instances_dir, &data, known_instances)
}

pub fn move_instance_in_group(
    instances_dir: &PathBuf,
    instance_id: &str,
    direction: &str,
    known_instances: &HashSet<String>,
) -> Result<(), String> {
    if !known_instances.contains(instance_id) {
        return Err("instance not found".into());
    }

    let mut data = load_group_data(instances_dir, known_instances);
    reconcile_group_orders(&mut data, known_instances);

    let group = data
        .instance_index
        .get(instance_id)
        .cloned()
        .unwrap_or_default();
    let order = data
        .group_order
        .get_mut(&group)
        .ok_or_else(|| "instance order not found".to_string())?;
    let idx = order
        .iter()
        .position(|id| id == instance_id)
        .ok_or_else(|| "instance not found in group order".to_string())?;

    match direction {
        "up" if idx > 0 => order.swap(idx, idx - 1),
        "down" if idx + 1 < order.len() => order.swap(idx, idx + 1),
        "up" | "down" => return Ok(()),
        other => return Err(format!("invalid direction: {other}")),
    }

    save_group_data(instances_dir, &data, known_instances)
}

fn group_member_ids(
    data: &GroupData,
    group: &str,
    known_instances: &HashSet<String>,
) -> HashSet<String> {
    known_instances
        .iter()
        .filter(|id| {
            data.instance_index
                .get(*id)
                .cloned()
                .unwrap_or_default()
                == group
        })
        .cloned()
        .collect()
}

pub fn set_group_instance_order(
    instances_dir: &PathBuf,
    group: &str,
    order: &[String],
    known_instances: &HashSet<String>,
) -> Result<(), String> {
    let mut data = load_group_data(instances_dir, known_instances);
    reconcile_group_orders(&mut data, known_instances);

    let members = group_member_ids(&data, group, known_instances);
    if members.is_empty() {
        return Ok(());
    }

    let mut next_order: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for id in order {
        if members.contains(id) && seen.insert(id.clone()) {
            next_order.push(id.clone());
        }
    }

    let in_order: HashSet<_> = next_order.iter().cloned().collect();
    let mut missing: Vec<_> = members
        .iter()
        .filter(|id| !in_order.contains(*id))
        .cloned()
        .collect();
    missing.sort();
    next_order.extend(missing);

    data.group_order.insert(group.to_string(), next_order);
    save_group_data(instances_dir, &data, known_instances)
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

    if let Some(order) = data.group_order.remove(old_name) {
        data.group_order.insert(new_name.to_string(), order);
    }

    let was_collapsed = data.collapsed.remove(old_name);
    if was_collapsed {
        data.collapsed.insert(new_name.to_string());
    }

    save_group_data(instances_dir, &data, known_instances)
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
    let moved: Vec<String> = data
        .instance_index
        .iter()
        .filter_map(|(id, group)| (group == name).then_some(id.clone()))
        .collect();

    for instance_id in &moved {
        data.instance_index.remove(instance_id);
        remove_from_group_order(&mut data, instance_id, name);
        append_to_group_order(&mut data, instance_id, "");
    }

    data.group_order.remove(name);
    data.collapsed.remove(name);
    save_group_data(instances_dir, &data, known_instances)
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
    save_group_data(instances_dir, &data, known_instances)
}

pub fn remove_instance_from_groups(
    instances_dir: &PathBuf,
    instance_id: &str,
    known_instances: &HashSet<String>,
) -> Result<(), String> {
    let mut data = load_group_data(instances_dir, known_instances);
    let old_group = data
        .instance_index
        .get(instance_id)
        .cloned()
        .unwrap_or_default();
    data.instance_index.remove(instance_id);
    remove_from_group_order(&mut data, instance_id, &old_group);
    remove_from_group_order(&mut data, instance_id, "");
    save_group_data(instances_dir, &data, known_instances)
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

    #[test]
    fn move_instance_preserves_custom_order() {
        let dir = temp_instances_dir();
        let known = HashSet::from([
            "inst-a".to_string(),
            "inst-b".to_string(),
            "inst-c".to_string(),
        ]);

        set_instance_group(&dir, "inst-a", "Pack", &known).unwrap();
        set_instance_group(&dir, "inst-b", "Pack", &known).unwrap();
        set_instance_group(&dir, "inst-c", "Pack", &known).unwrap();

        move_instance_in_group(&dir, "inst-c", "up", &known).unwrap();
        move_instance_in_group(&dir, "inst-c", "up", &known).unwrap();

        let state = get_groups_state(&dir, &known);
        assert_eq!(
            state.instance_order.get("Pack").map(|v| v.as_slice()),
            Some(
                ["inst-c".to_string(), "inst-a".to_string(), "inst-b".to_string()].as_slice()
            )
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn set_group_instance_order_applies_custom_sequence() {
        let dir = temp_instances_dir();
        let known = HashSet::from([
            "inst-a".to_string(),
            "inst-b".to_string(),
            "inst-c".to_string(),
        ]);

        set_instance_group(&dir, "inst-a", "Pack", &known).unwrap();
        set_instance_group(&dir, "inst-b", "Pack", &known).unwrap();
        set_instance_group(&dir, "inst-c", "Pack", &known).unwrap();

        set_group_instance_order(
            &dir,
            "Pack",
            &[
                "inst-c".to_string(),
                "inst-a".to_string(),
                "inst-b".to_string(),
            ],
            &known,
        )
        .unwrap();

        let state = get_groups_state(&dir, &known);
        assert_eq!(
            state.instance_order.get("Pack").map(|v| v.as_slice()),
            Some(
                ["inst-c".to_string(), "inst-a".to_string(), "inst-b".to_string()].as_slice()
            )
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_group_instance_order_deduplicates_ids() {
        let dir = temp_instances_dir();
        let known = HashSet::from([
            "inst-a".to_string(),
            "inst-b".to_string(),
            "inst-c".to_string(),
        ]);

        set_instance_group(&dir, "inst-a", "Pack", &known).unwrap();
        set_instance_group(&dir, "inst-b", "Pack", &known).unwrap();
        set_instance_group(&dir, "inst-c", "Pack", &known).unwrap();

        set_group_instance_order(
            &dir,
            "Pack",
            &[
                "inst-b".to_string(),
                "inst-b".to_string(),
                "inst-a".to_string(),
                "inst-c".to_string(),
                "inst-a".to_string(),
            ],
            &known,
        )
        .unwrap();

        let state = get_groups_state(&dir, &known);
        assert_eq!(
            state.instance_order.get("Pack").map(|v| v.as_slice()),
            Some(
                ["inst-b".to_string(), "inst-a".to_string(), "inst-c".to_string()].as_slice()
            )
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn instance_order_is_not_alphabetized_on_save() {
        let dir = temp_instances_dir();
        let known = HashSet::from(["z-inst".to_string(), "a-inst".to_string()]);

        set_instance_group(&dir, "z-inst", "Pack", &known).unwrap();
        set_instance_group(&dir, "a-inst", "Pack", &known).unwrap();

        let state = get_groups_state(&dir, &known);
        assert_eq!(
            state.instance_order.get("Pack").map(|v| v.as_slice()),
            Some(["z-inst".to_string(), "a-inst".to_string()].as_slice())
        );

        let _ = fs::remove_dir_all(dir);
    }
}

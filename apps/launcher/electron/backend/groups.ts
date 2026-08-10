import { groupsPath, instancesDir } from "./paths";
import { readJson, writeJson } from "./fs-utils";
import type { InstanceGroupsState } from "./types";

interface GroupEntry {
  hidden?: boolean;
  instances?: string[];
}

interface GroupFile {
  formatVersion?: number;
  groups?: Record<string, GroupEntry>;
  ungrouped?: GroupEntry & { name?: string };
}

interface GroupData {
  instanceIndex: Map<string, string>;
  groupOrder: Map<string, string[]>;
  collapsed: Set<string>;
  ungroupedName: string;
}

const defaultUngroupedName = "Ungrouped";

async function loadData(known: Set<string>): Promise<GroupData> {
  const file = (await readJson<GroupFile>(groupsPath())) ?? {};
  const ungrouped = file.ungrouped ?? {};
  const instanceIndex = new Map<string, string>();
  const groupOrder = new Map<string, string[]>();
  const collapsed = new Set<string>();
  if (ungrouped.hidden) collapsed.add("");
  const ungroupedName = ungrouped.name?.trim() || defaultUngroupedName;
  const ungroupedOrder = (ungrouped.instances ?? []).filter((id) => known.has(id));
  groupOrder.set("", [...ungroupedOrder]);

  for (const [name, entry] of Object.entries(file.groups ?? {})) {
    if (entry.hidden) collapsed.add(name);
    const ids = (entry.instances ?? []).filter((id) => known.has(id));
    for (const id of ids) instanceIndex.set(id, name);
    groupOrder.set(name, ids);
  }

  for (const id of known) {
    if (!instanceIndex.has(id)) ungroupedOrder.push(id);
  }
  groupOrder.set("", ungroupedOrder);
  reconcile({ instanceIndex, groupOrder, collapsed, ungroupedName }, known);
  return { instanceIndex, groupOrder, collapsed, ungroupedName };
}

function reconcile(data: GroupData, known: Set<string>): void {
  const members = new Map<string, Set<string>>();
  for (const id of known) {
    const group = data.instanceIndex.get(id) ?? "";
    const set = members.get(group) ?? new Set<string>();
    set.add(id);
    members.set(group, set);
  }
  for (const [group, memberSet] of members) {
    const order = data.groupOrder.get(group) ?? [];
    const seen = new Set<string>();
    const next = order.filter((id) => memberSet.has(id) && !seen.has(id) && seen.add(id));
    next.push(...[...memberSet].filter((id) => !seen.has(id)).sort());
    data.groupOrder.set(group, next);
  }
  for (const [group, order] of data.groupOrder) {
    if (!members.has(group) && order.length === 0) data.groupOrder.delete(group);
  }
}

async function saveData(data: GroupData, known: Set<string>): Promise<void> {
  const groups: Record<string, GroupEntry> = {};
  for (const [name, order] of data.groupOrder) {
    if (!name) continue;
    const ids = order.filter((id, index) => {
      return known.has(id) && data.instanceIndex.get(id) === name && order.indexOf(id) === index;
    });
    if (ids.length || data.collapsed.has(name)) {
      groups[name] = { hidden: data.collapsed.has(name), instances: ids };
    }
  }
  const seen = new Set<string>();
  const ungrouped = (data.groupOrder.get("") ?? []).filter((id) => {
    if (!known.has(id) || data.instanceIndex.has(id) || seen.has(id)) return false;
    seen.add(id);
    return true;
  });
  await writeJson(groupsPath(), {
    formatVersion: 1,
    groups,
    ungrouped: {
      hidden: data.collapsed.has(""),
      instances: ungrouped,
      name: data.ungroupedName,
    },
  });
}

export async function getInstanceGroup(id: string, known: Set<string>): Promise<string> {
  return (await loadData(known)).instanceIndex.get(id) ?? "";
}

export async function getGroupsState(known: Set<string>): Promise<InstanceGroupsState> {
  const data = await loadData(known);
  const groups = [...new Set(data.instanceIndex.values())].sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()));
  const collapsed: Record<string, boolean> = {};
  for (const group of groups) collapsed[group] = data.collapsed.has(group);
  collapsed[""] = data.collapsed.has("");
  return {
    collapsed,
    groups,
    instance_order: Object.fromEntries(data.groupOrder),
    ungrouped_name: data.ungroupedName,
  };
}

export async function setInstanceGroup(id: string, group: string, known: Set<string>): Promise<void> {
  if (!known.has(id)) throw new Error("instance not found");
  const name = group.trim();
  if (name.length > 128) throw new Error("group name too long");
  const data = await loadData(known);
  const old = data.instanceIndex.get(id) ?? "";
  if (name) data.instanceIndex.set(id, name);
  else data.instanceIndex.delete(id);
  removeFromOrder(data, id, old);
  appendToOrder(data, id, name);
  await saveData(data, known);
}

export async function moveInstanceInGroup(id: string, direction: string, known: Set<string>): Promise<void> {
  if (!known.has(id)) throw new Error("instance not found");
  const data = await loadData(known);
  const group = data.instanceIndex.get(id) ?? "";
  const order = data.groupOrder.get(group);
  if (!order) throw new Error("instance order not found");
  const index = order.indexOf(id);
  if (index < 0) throw new Error("instance not found in group order");
  if (direction === "up" && index > 0) [order[index - 1], order[index]] = [order[index], order[index - 1]];
  else if (direction === "down" && index + 1 < order.length) [order[index], order[index + 1]] = [order[index + 1], order[index]];
  else if (direction !== "up" && direction !== "down") throw new Error(`invalid direction: ${direction}`);
  await saveData(data, known);
}

export async function setGroupInstanceOrder(group: string, order: string[], known: Set<string>): Promise<void> {
  const data = await loadData(known);
  const members = new Set([...known].filter((id) => (data.instanceIndex.get(id) ?? "") === group));
  if (!members.size) return;
  const seen = new Set<string>();
  const next = order.filter((id) => members.has(id) && !seen.has(id) && seen.add(id));
  next.push(...[...members].filter((id) => !seen.has(id)).sort());
  data.groupOrder.set(group, next);
  await saveData(data, known);
}

export async function renameGroup(oldName: string, newName: string, known: Set<string>): Promise<void> {
  const old = oldName.trim();
  const next = newName.trim();
  if (!next) throw new Error("group name cannot be empty");
  if (next.length > 128) throw new Error("group name too long");
  const data = await loadData(known);
  if (!old) {
    if (data.ungroupedName.toLowerCase() === next.toLowerCase()) return;
    if ([...data.instanceIndex.values()].some((group) => group.toLowerCase() === next.toLowerCase())) throw new Error("a group with that name already exists");
    data.ungroupedName = next;
    await saveData(data, known);
    return;
  }
  if (old.toLowerCase() === next.toLowerCase()) return;
  if (![...data.instanceIndex.values()].some((group) => group === old)) throw new Error("group not found");
  if (
    data.ungroupedName.toLowerCase() === next.toLowerCase() ||
    [...data.instanceIndex.values()].some((group) => group.toLowerCase() === next.toLowerCase() && group !== old)
  )
    throw new Error("a group with that name already exists");
  for (const [id, group] of data.instanceIndex) if (group === old) data.instanceIndex.set(id, next);
  const order = data.groupOrder.get(old);
  if (order) data.groupOrder.set(next, order);
  data.groupOrder.delete(old);
  if (data.collapsed.delete(old)) data.collapsed.add(next);
  await saveData(data, known);
}

export async function deleteGroup(name: string, known: Set<string>): Promise<void> {
  const group = name.trim();
  if (!group) throw new Error("cannot delete ungrouped section");
  const data = await loadData(known);
  for (const [id, current] of data.instanceIndex) {
    if (current === group) {
      data.instanceIndex.delete(id);
      removeFromOrder(data, id, group);
      appendToOrder(data, id, "");
    }
  }
  data.groupOrder.delete(group);
  data.collapsed.delete(group);
  await saveData(data, known);
}

export async function setGroupCollapsed(group: string, collapsed: boolean, known: Set<string>): Promise<void> {
  const data = await loadData(known);
  if (collapsed) data.collapsed.add(group);
  else data.collapsed.delete(group);
  await saveData(data, known);
}

export async function removeInstanceFromGroups(id: string, known: Set<string>): Promise<void> {
  const data = await loadData(known);
  const group = data.instanceIndex.get(id) ?? "";
  data.instanceIndex.delete(id);
  removeFromOrder(data, id, group);
  removeFromOrder(data, id, "");
  await saveData(data, known);
}

function removeFromOrder(data: GroupData, id: string, group: string): void {
  data.groupOrder.set(
    group,
    (data.groupOrder.get(group) ?? []).filter((entry) => entry !== id),
  );
}

function appendToOrder(data: GroupData, id: string, group: string): void {
  const order = data.groupOrder.get(group) ?? [];
  if (!order.includes(id)) order.push(id);
  data.groupOrder.set(group, order);
}

export { instancesDir };

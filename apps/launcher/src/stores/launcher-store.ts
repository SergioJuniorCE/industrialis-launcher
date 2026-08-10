import { create } from "zustand";
import type { BackgroundProcess } from "../lib/background-processes";
import type { InstanceSettings } from "../lib/instance-settings";

export interface GtnhVersion {
  title: string;
  description: string;
  releaseDate: string;
  maxJavaVersion: number;
  mmc: { java8Url: string; java17_2XUrl: string };
  client: { java8Url: string };
}

export interface InstanceInfo {
  id: string;
  installed: boolean;
  size_bytes: number;
  settings: InstanceSettings;
  group: string;
  icon_path?: string | null;
}

export interface InstanceGroupsState {
  collapsed: Record<string, boolean>;
  groups: string[];
  instance_order: Record<string, string[]>;
  ungrouped_name: string;
}

export interface LauncherAccount {
  id: string;
  username: string;
  uuid: string;
  account_type: string;
  skin_png_base64?: string;
  owns_minecraft?: boolean;
  can_play_minecraft?: boolean;
}

type StateUpdate<T> = T | ((current: T) => T);

function resolveUpdate<T>(current: T, update: StateUpdate<T>): T {
  return typeof update === "function" ? (update as (value: T) => T)(current) : update;
}

function createInitialState() {
  return {
    tab: "instances",
    selectedProcessKey: null as string | null,
    selectedInstanceId: null as string | null,
    showNewInstance: false,
    detailTab: "info",
    instances: [] as InstanceInfo[],
    sizesRefreshing: false,
    processes: new Map<string, BackgroundProcess>(),
    launching: null as string | null,
    runningInstanceIds: new Set<string>(),
    groupsState: {
      collapsed: {},
      groups: [],
      instance_order: {},
      ungrouped_name: "Ungrouped",
    } as InstanceGroupsState,
    accounts: [] as LauncherAccount[],
    accountsLoaded: false,
    gtnhVersions: null as Record<string, GtnhVersion> | null,
    updatePackInstanceId: null as string | null,
    reinstallInstanceId: null as string | null,
    copyInstanceId: null as string | null,
    changeGroupInstanceId: null as string | null,
    renameInstanceId: null as string | null,
  };
}

export interface LauncherStoreState {
  tab: string;
  selectedProcessKey: string | null;
  selectedInstanceId: string | null;
  showNewInstance: boolean;
  detailTab: string;
  instances: InstanceInfo[];
  sizesRefreshing: boolean;
  processes: Map<string, BackgroundProcess>;
  launching: string | null;
  runningInstanceIds: Set<string>;
  groupsState: InstanceGroupsState;
  accounts: LauncherAccount[];
  accountsLoaded: boolean;
  gtnhVersions: Record<string, GtnhVersion> | null;
  updatePackInstanceId: string | null;
  reinstallInstanceId: string | null;
  copyInstanceId: string | null;
  changeGroupInstanceId: string | null;
  renameInstanceId: string | null;
  setTab: (update: StateUpdate<string>) => void;
  setSelectedProcessKey: (update: StateUpdate<string | null>) => void;
  setSelectedInstanceId: (update: StateUpdate<string | null>) => void;
  setShowNewInstance: (update: StateUpdate<boolean>) => void;
  setDetailTab: (update: StateUpdate<string>) => void;
  setInstances: (update: StateUpdate<InstanceInfo[]>) => void;
  setSizesRefreshing: (update: StateUpdate<boolean>) => void;
  setProcesses: (update: StateUpdate<Map<string, BackgroundProcess>>) => void;
  setLaunching: (update: StateUpdate<string | null>) => void;
  setRunningInstanceIds: (update: StateUpdate<Set<string>>) => void;
  setGroupsState: (update: StateUpdate<InstanceGroupsState>) => void;
  setAccounts: (update: StateUpdate<LauncherAccount[]>) => void;
  setAccountsLoaded: (update: StateUpdate<boolean>) => void;
  setGtnhVersions: (update: StateUpdate<Record<string, GtnhVersion> | null>) => void;
  setUpdatePackInstanceId: (update: StateUpdate<string | null>) => void;
  setReinstallInstanceId: (update: StateUpdate<string | null>) => void;
  setCopyInstanceId: (update: StateUpdate<string | null>) => void;
  setChangeGroupInstanceId: (update: StateUpdate<string | null>) => void;
  setRenameInstanceId: (update: StateUpdate<string | null>) => void;
  openInstanceSettings: (id: string) => void;
}

export const useLauncherStore = create<LauncherStoreState>()((set) => ({
  ...createInitialState(),
  setTab: (update) => set((state) => ({ tab: resolveUpdate(state.tab, update) })),
  setSelectedProcessKey: (update) => set((state) => ({ selectedProcessKey: resolveUpdate(state.selectedProcessKey, update) })),
  setSelectedInstanceId: (update) => set((state) => ({ selectedInstanceId: resolveUpdate(state.selectedInstanceId, update) })),
  setShowNewInstance: (update) => set((state) => ({ showNewInstance: resolveUpdate(state.showNewInstance, update) })),
  setDetailTab: (update) => set((state) => ({ detailTab: resolveUpdate(state.detailTab, update) })),
  setInstances: (update) => set((state) => ({ instances: resolveUpdate(state.instances, update) })),
  setSizesRefreshing: (update) => set((state) => ({ sizesRefreshing: resolveUpdate(state.sizesRefreshing, update) })),
  setProcesses: (update) => set((state) => ({ processes: resolveUpdate(state.processes, update) })),
  setLaunching: (update) => set((state) => ({ launching: resolveUpdate(state.launching, update) })),
  setRunningInstanceIds: (update) => set((state) => ({ runningInstanceIds: resolveUpdate(state.runningInstanceIds, update) })),
  setGroupsState: (update) => set((state) => ({ groupsState: resolveUpdate(state.groupsState, update) })),
  setAccounts: (update) => set((state) => ({ accounts: resolveUpdate(state.accounts, update) })),
  setAccountsLoaded: (update) => set((state) => ({ accountsLoaded: resolveUpdate(state.accountsLoaded, update) })),
  setGtnhVersions: (update) => set((state) => ({ gtnhVersions: resolveUpdate(state.gtnhVersions, update) })),
  setUpdatePackInstanceId: (update) => set((state) => ({ updatePackInstanceId: resolveUpdate(state.updatePackInstanceId, update) })),
  setReinstallInstanceId: (update) => set((state) => ({ reinstallInstanceId: resolveUpdate(state.reinstallInstanceId, update) })),
  setCopyInstanceId: (update) => set((state) => ({ copyInstanceId: resolveUpdate(state.copyInstanceId, update) })),
  setChangeGroupInstanceId: (update) => set((state) => ({ changeGroupInstanceId: resolveUpdate(state.changeGroupInstanceId, update) })),
  setRenameInstanceId: (update) => set((state) => ({ renameInstanceId: resolveUpdate(state.renameInstanceId, update) })),
  openInstanceSettings: (id) => set({ selectedInstanceId: id, detailTab: "settings" }),
}));

export function resetLauncherStore(): void {
  useLauncherStore.setState(createInitialState());
}

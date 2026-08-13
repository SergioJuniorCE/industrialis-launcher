import { useEffect, useMemo, useSyncExternalStore } from "react";
import type { LauncherUpdateState } from "./launcher-update";
import {
  applyDlProgressEvent,
  createProcess,
  dismissProcess as dismissBackgroundProcess,
  markProcessFailed,
  operationLabel,
  resolveOperation,
  type DlProgressEvent,
  type ProcessOperation,
} from "./background-processes";
import { appendLogTail, MAX_RETAINED_LOG_LINES, takeLogTail } from "./log-buffer";
import type { LaunchLogLine } from "./launch-log";
import type { InstanceSettings } from "./instance-settings";
import type { JavaInfo } from "./java-installations";
import { hideWindow, invoke, isDesktop, listen } from "./desktop";
import type { GtnhVersion, InstanceGroupsState, InstanceInfo, LauncherAccount, LauncherStoreState } from "../stores/launcher-store";
import { useLauncherStore } from "../stores/launcher-store";

const LOG_BATCH_DELAY_MS = 50;

export interface LaunchLogEvent extends LaunchLogLine {
  id: string;
}

export interface LauncherSessionSnapshot {
  error: string | null;
  javaOptions: JavaInfo[];
  javaRefreshing: boolean;
  instanceLogs: Record<string, LaunchLogLine[]>;
  launcherUpdate: LauncherUpdateState;
}

export interface LauncherSessionDesktop {
  invoke<T>(command: string, args?: unknown): Promise<T>;
  listen<T>(event: string, listener: (event: { payload: T }) => void): Promise<() => void>;
  hideWindow(): Promise<void>;
}

export interface LauncherSessionStore {
  getState(): LauncherStoreState;
  setState(update: Partial<LauncherStoreState> | ((state: LauncherStoreState) => Partial<LauncherStoreState>)): void;
}

export interface LauncherSession {
  readonly snapshot: LauncherSessionSnapshot;
  subscribe(listener: () => void): () => void;
  start(): Promise<void>;
  dispose(): void;
  setError(message: string | null): void;
  loadGroups(): Promise<void>;
  loadInstances(): Promise<void>;
  refreshJava(): Promise<JavaInfo[]>;
  loadLogs(id: string): Promise<void>;
  getConsoleLog(id: string, full?: boolean): Promise<LaunchLogLine[]>;
  clearConsole(id: string): Promise<void>;
  launch(id: string, settings: InstanceSettings | null): Promise<void>;
  kill(id: string): Promise<void>;
  startProcess(operation: ProcessOperation, id: string, name: string, initialLog?: string): string;
  failProcess(operation: ProcessOperation, id: string, error: unknown): void;
  dismissProcess(key: string): void;
  installLauncherUpdate(): Promise<void>;
  retryLauncherUpdate(): Promise<void>;
  dismissLauncherUpdate(): void;
}

export interface UseLauncherSessionResult extends LauncherSessionSnapshot {
  session: LauncherSession;
}

interface CreateLauncherSessionOptions {
  desktop: LauncherSessionDesktop;
  store: LauncherSessionStore;
}

const INITIAL_UPDATE_STATE: LauncherUpdateState = {
  status: "idle",
  current_version: "",
};

function initialSnapshot(): LauncherSessionSnapshot {
  return {
    error: null,
    javaOptions: [],
    javaRefreshing: false,
    instanceLogs: {},
    launcherUpdate: INITIAL_UPDATE_STATE,
  };
}

function formatError(error: unknown): string {
  return String(error);
}

export function createLauncherSession({ desktop, store }: CreateLauncherSessionOptions): LauncherSession {
  let currentSnapshot = initialSnapshot();
  let disposed = false;
  let started = false;
  let startPromise: Promise<void> | null = null;
  let lifecycle = 0;
  let flushTimer: ReturnType<typeof setTimeout> | null = null;
  let javaRequest = 0;
  const pendingLogs = new Map<string, LaunchLogLine[]>();
  const logRequests = new Map<string, number>();
  const subscribers = new Set<() => void>();
  const unsubscribers = new Set<() => void>();
  const isCurrent = (generation: number) => !disposed && lifecycle === generation;

  const notify = () => {
    for (const listener of subscribers) listener();
  };

  const updateSnapshot = (update: Partial<LauncherSessionSnapshot>) => {
    currentSnapshot = { ...currentSnapshot, ...update };
    notify();
  };

  const setError = (message: string | null) => {
    updateSnapshot({ error: message });
  };

  const setLogs = (update: (current: Record<string, LaunchLogLine[]>) => Record<string, LaunchLogLine[]>) => {
    updateSnapshot({ instanceLogs: update(currentSnapshot.instanceLogs) });
  };

  const removeLog = (id: string) => {
    setLogs((current) => {
      if (!(id in current)) return current;
      const next = { ...current };
      delete next[id];
      return next;
    });
  };

  const flushPendingLogs = () => {
    flushTimer = null;
    if (disposed || pendingLogs.size === 0) return;

    const batch = new Map(pendingLogs);
    pendingLogs.clear();
    setLogs((current) => {
      const next = { ...current };
      for (const [id, entries] of batch) {
        next[id] = appendLogTail(current[id] ?? [], entries);
      }
      return next;
    });
  };

  const scheduleLogFlush = () => {
    if (flushTimer === null) {
      flushTimer = setTimeout(flushPendingLogs, LOG_BATCH_DELAY_MS);
    }
  };

  const handleLaunchLog = (event: { payload: LaunchLogEvent }) => {
    if (disposed) return;
    const { id, stream, line } = event.payload;
    const entries = pendingLogs.get(id) ?? [];
    entries.push({ stream, line });
    if (entries.length > MAX_RETAINED_LOG_LINES) {
      entries.splice(0, entries.length - MAX_RETAINED_LOG_LINES);
    }
    pendingLogs.set(id, entries);
    scheduleLogFlush();
  };

  const handleInstanceStarted = (event: { payload: { id: string } }) => {
    if (disposed) return;
    const { id } = event.payload;
    store.setState((state) => ({
      runningInstanceIds: new Set(state.runningInstanceIds).add(id),
      ...(state.launching === id ? { launching: null } : {}),
    }));
  };

  const handleInstanceStopped = (event: { payload: { id: string } }) => {
    if (disposed) return;
    const { id } = event.payload;
    store.setState((state) => {
      const runningInstanceIds = new Set(state.runningInstanceIds);
      runningInstanceIds.delete(id);
      return {
        runningInstanceIds,
        ...(state.launching === id ? { launching: null } : {}),
      };
    });
    void loadInstances();
  };

  const handleDownloadProgress = (event: { payload: DlProgressEvent }) => {
    if (disposed) return;
    const progress = event.payload;
    const previous = store.getState().processes;
    const operation = resolveOperation(previous, progress);
    const next = applyDlProgressEvent(previous, progress);
    store.setState({ processes: next });

    if (progress.stage === "failed" && progress.id && operation) {
      const message = progress.log_line?.replace(/^Error:\s*/u, "") ?? "Unknown error";
      setError(`${operationLabel(operation)} failed: ${message}`);
    }

    if (progress.stage !== "done" || !progress.id) return;

    if (operation === "delete") {
      store.setState((state) => ({
        selectedInstanceId: state.selectedInstanceId === progress.id ? null : state.selectedInstanceId,
      }));
      removeLog(progress.id);
    } else if (operation === "install") {
      store.setState({
        selectedInstanceId: progress.id,
        showNewInstance: false,
      });
    } else if (operation === "update-pack" || operation === "reinstall" || operation === "copy") {
      store.setState({
        selectedInstanceId: progress.id,
        tab: "instances",
        selectedProcessKey: null,
      });
    }

    void loadInstances();
  };

  const attachListener = <T>(event: string, listener: (event: { payload: T }) => void, generation: number): void => {
    const pendingUnsubscriber = desktop.listen(event, listener);
    void pendingUnsubscriber.then(
      (unsubscribe) => {
        if (disposed || !started || lifecycle !== generation) {
          unsubscribe();
          return;
        }
        unsubscribers.add(unsubscribe);
      },
      () => undefined,
    );
  };

  const loadGroups = async (generation = lifecycle): Promise<void> => {
    try {
      const groups = await desktop.invoke<InstanceGroupsState>("get_instance_groups");
      if (isCurrent(generation)) store.setState({ groupsState: groups });
    } catch {
      // Group data is supplementary; the instance list remains usable without it.
    }
  };

  const loadAccounts = async (generation = lifecycle): Promise<void> => {
    try {
      const accounts = await desktop.invoke<LauncherAccount[]>("get_accounts");
      if (isCurrent(generation)) store.setState({ accounts });
    } catch {
      if (isCurrent(generation)) store.setState({ accounts: [] });
    } finally {
      if (isCurrent(generation)) store.setState({ accountsLoaded: true });
    }
  };

  const loadInstanceSizes = async (generation = lifecycle): Promise<void> => {
    if (!isCurrent(generation)) return;
    store.setState({ sizesRefreshing: true });
    try {
      const sizes = await desktop.invoke<Record<string, number>>("refresh_instance_sizes", { ids: null });
      if (!isCurrent(generation)) return;
      store.setState((state) => ({
        instances: state.instances.map((instance) => ({
          ...instance,
          size_bytes: sizes[instance.id] ?? instance.size_bytes,
        })),
      }));
    } catch {
      // Size refresh is best effort and should not hide the instance list.
    } finally {
      if (isCurrent(generation)) store.setState({ sizesRefreshing: false });
    }
  };

  const loadInstances = async (generation = lifecycle): Promise<void> => {
    try {
      const instances = await desktop.invoke<InstanceInfo[]>("get_instances");
      if (!isCurrent(generation)) return;
      store.setState({ instances });
      void loadGroups(generation);
      if (instances.some((instance) => instance.size_bytes === 0)) {
        void loadInstanceSizes(generation);
      }
    } catch {
      // The existing list is safer than replacing it after a transient bridge failure.
    }
  };

  const refreshJava = async (generation = lifecycle): Promise<JavaInfo[]> => {
    const request = ++javaRequest;
    updateSnapshot({ javaRefreshing: true });
    try {
      const detected = await desktop.invoke<JavaInfo[]>("detect_java");
      if (isCurrent(generation) && request === javaRequest) updateSnapshot({ javaOptions: detected });
      return detected;
    } catch (error) {
      if (isCurrent(generation) && request === javaRequest) setError(`Java detection failed: ${formatError(error)}`);
      return [];
    } finally {
      if (isCurrent(generation) && request === javaRequest) updateSnapshot({ javaRefreshing: false });
    }
  };

  const getConsoleLog = async (id: string, full = false): Promise<LaunchLogLine[]> =>
    desktop.invoke<LaunchLogLine[]>("get_instance_console_log", {
      id,
      ...(full ? { full: true } : {}),
    });

  const loadLogs = async (id: string, generation = lifecycle): Promise<void> => {
    const request = (logRequests.get(id) ?? 0) + 1;
    logRequests.set(id, request);
    try {
      const persisted = await getConsoleLog(id);
      if (!isCurrent(generation) || logRequests.get(id) !== request) return;
      const launcherState = store.getState();
      const instanceIsActive = launcherState.launching === id || launcherState.runningInstanceIds.has(id);
      if (instanceIsActive && (currentSnapshot.instanceLogs[id]?.length ?? 0) > 0) return;
      setLogs((current) => ({ ...current, [id]: takeLogTail(persisted) }));
    } catch {
      // Keep live in-memory logs if persisted log loading fails.
    }
  };

  const clearConsole = async (id: string): Promise<void> => {
    try {
      await desktop.invoke("clear_instance_console_log", { id });
      removeLog(id);
    } catch (error) {
      setError(`Clear console failed: ${formatError(error)}`);
    }
  };

  const launch = async (id: string, settings: InstanceSettings | null): Promise<void> => {
    if (store.getState().launching !== null) return;
    setError(null);
    store.setState({ selectedInstanceId: id, launching: id });

    const consoleConfig = settings?.override_console
      ? {
          showOnLaunch: settings.show_console_on_launch,
          showOnError: settings.show_console_on_error,
          autoClose: settings.auto_close_console,
        }
      : { showOnLaunch: false, showOnError: true, autoClose: false };

    if (consoleConfig.showOnLaunch) {
      store.setState({ detailTab: "logs" });
    }

    if (settings?.override_window && settings.close_after_launch) {
      try {
        await desktop.hideWindow();
      } catch {
        // Browser-only renderer previews have no native window bridge.
      }
    }

    try {
      await desktop.invoke("launch_instance", { id });
      if (settings?.override_window && settings.quit_after_game_stop) {
        try {
          await desktop.invoke("exit_launcher");
        } catch {
          // Browser-only renderer previews have no native window bridge.
        }
      }
      if (consoleConfig.autoClose) {
        store.setState({ detailTab: "info" });
      }
      void loadInstances();
    } catch (error) {
      if (consoleConfig.showOnError) {
        store.setState({ detailTab: "logs" });
      }
      setError(`Launch failed: ${formatError(error)}`);
    } finally {
      store.setState({ launching: null });
    }
  };

  const kill = async (id: string): Promise<void> => {
    try {
      await desktop.invoke("kill_instance", { id });
    } catch (error) {
      setError(`Stop failed: ${formatError(error)}`);
    } finally {
      store.setState((state) => {
        const runningInstanceIds = new Set(state.runningInstanceIds);
        runningInstanceIds.delete(id);
        return {
          runningInstanceIds,
          ...(state.launching === id ? { launching: null } : {}),
        };
      });
    }
  };

  const startProcess = (operation: ProcessOperation, id: string, name: string, initialLog?: string): string => {
    const process = createProcess(operation, id, name, initialLog);
    store.setState((state) => {
      const processes = new Map(state.processes);
      processes.set(process.key, process);
      return { processes };
    });
    return process.key;
  };

  const failProcess = (operation: ProcessOperation, id: string, error: unknown): void => {
    store.setState((state) => ({
      processes: markProcessFailed(state.processes, operation, id, error),
    }));
    setError(`${operationLabel(operation)} failed: ${formatError(error)}`);
  };

  const dismissProcess = (key: string): void => {
    store.setState((state) => ({
      processes: dismissBackgroundProcess(state.processes, key),
    }));
  };

  const updateLauncherState = (state: LauncherUpdateState, generation = lifecycle) => {
    if (isCurrent(generation)) updateSnapshot({ launcherUpdate: state });
  };

  const installLauncherUpdate = async (): Promise<void> => {
    const generation = lifecycle;
    try {
      updateLauncherState(await desktop.invoke<LauncherUpdateState>("install_launcher_update"), generation);
    } catch (error) {
      if (isCurrent(generation)) {
        updateSnapshot({
          launcherUpdate: {
            ...currentSnapshot.launcherUpdate,
            status: "failed",
            error: formatError(error),
          },
        });
      }
    }
  };

  const retryLauncherUpdate = async (): Promise<void> => {
    const generation = lifecycle;
    try {
      updateLauncherState(await desktop.invoke<LauncherUpdateState>("check_launcher_update"), generation);
    } catch (error) {
      if (isCurrent(generation)) {
        updateSnapshot({
          launcherUpdate: {
            ...currentSnapshot.launcherUpdate,
            status: "failed",
            error: formatError(error),
          },
        });
      }
    }
  };

  const dismissLauncherUpdate = () => {
    updateLauncherState({ ...currentSnapshot.launcherUpdate, status: "idle" });
  };

  const start = (): Promise<void> => {
    if (startPromise) return startPromise;
    disposed = false;
    started = true;
    const generation = ++lifecycle;

    attachListener<LauncherUpdateState>("launcher-update", (event) => updateLauncherState(event.payload), generation);
    attachListener<LaunchLogEvent>("launch-log", handleLaunchLog, generation);
    attachListener<{ id: string }>("instance-started", handleInstanceStarted, generation);
    attachListener<{ id: string }>("instance-stopped", handleInstanceStopped, generation);
    attachListener<DlProgressEvent>("dl-progress", handleDownloadProgress, generation);

    startPromise = Promise.all([
      loadInstances(generation),
      loadAccounts(generation),
      refreshJava(generation),
      desktop
        .invoke<Record<string, GtnhVersion>>("get_versions")
        .then((gtnhVersions) => {
          if (isCurrent(generation)) store.setState({ gtnhVersions });
        })
        .catch(() => undefined),
      desktop
        .invoke<LauncherUpdateState>("check_launcher_update")
        .then((state) => updateLauncherState(state, generation))
        .catch(() => undefined),
    ]).then(() => undefined);

    return startPromise;
  };

  const dispose = () => {
    disposed = true;
    started = false;
    startPromise = null;
    lifecycle += 1;
    if (flushTimer !== null) {
      clearTimeout(flushTimer);
      flushTimer = null;
    }
    pendingLogs.clear();
    logRequests.clear();
    for (const unsubscribe of unsubscribers) unsubscribe();
    unsubscribers.clear();
  };

  const session: LauncherSession = {
    get snapshot() {
      return currentSnapshot;
    },
    subscribe(listener) {
      subscribers.add(listener);
      return () => subscribers.delete(listener);
    },
    start,
    dispose,
    setError,
    loadGroups,
    loadInstances,
    refreshJava,
    loadLogs,
    getConsoleLog,
    clearConsole,
    launch,
    kill,
    startProcess,
    failProcess,
    dismissProcess,
    installLauncherUpdate,
    retryLauncherUpdate,
    dismissLauncherUpdate,
  };

  return session;
}

export function startLauncherSession(session: LauncherSession, desktopAvailable = isDesktop()): () => void {
  if (!desktopAvailable) {
    useLauncherStore.setState({ accountsLoaded: true });
    return () => session.dispose();
  }
  void session.start();
  return () => session.dispose();
}

export function useLauncherSession(): UseLauncherSessionResult {
  const session = useMemo(
    () =>
      createLauncherSession({
        desktop: { invoke, listen, hideWindow },
        store: useLauncherStore,
      }),
    [],
  );
  const snapshot = useSyncExternalStore(
    session.subscribe,
    () => session.snapshot,
    () => session.snapshot,
  );

  useEffect(() => startLauncherSession(session), [session]);

  return { ...snapshot, session };
}

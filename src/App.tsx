import { useEffect, useState, useCallback, useRef, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Button } from "./components/ui/button";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "./components/ui/card";
import { Input } from "./components/ui/input";
import { Badge } from "./components/ui/badge";
import { Progress } from "./components/ui/progress";
import { Textarea } from "./components/ui/textarea";
import { Select } from "./components/ui/select";
import { ScrollArea } from "./components/ui/scroll-area";
import { ThemeEditor } from "./components/ThemeEditor";
import { ThemeSwitcher } from "./components/ThemeSwitcher";
import "./App.css";

// ── Types ──

interface GtnhVersion {
  title: string;
  description: string;
  releaseDate: string;
  maxJavaVersion: number;
  mmc: { java8Url: string; java17_2XUrl: string };
  client: { java8Url: string };
}

interface InstanceInfo {
  version: string;
  installed: boolean;
  size_bytes: number;
  settings: InstanceSettings;
  group: string;
}

interface InstanceGroupsState {
  collapsed: Record<string, boolean>;
  groups: string[];
}

interface InstanceSettings {
  name: string;
  java_path: string | null;
  min_ram_mb: number;
  max_ram_mb: number;
  jvm_args: string;
  auth_mode: string;
  username: string;
}

interface JavaInfo {
  path: string;
  version: number;
}

interface DlProgress {
  stage: string;
  pct: number;
}

interface LaunchLogLine {
  stream: "stdout" | "stderr" | "system";
  line: string;
}

interface LaunchLogEvent extends LaunchLogLine {
  version: string;
}

interface AccountInfo {
  id: string;
  username: string;
  uuid: string;
  skin_png_base64?: string;
  owns_minecraft?: boolean;
}

interface DeviceCodeInfo {
  user_code: string;
  verification_uri: string;
  message: string;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function formatLaunchLog(log: LaunchLogLine[]): string {
  return log.map((entry) => entry.line).join("\n");
}

function LaunchConsole({
  version,
  instanceName,
  launching,
  log,
  onClear,
  onClose,
}: {
  version: string;
  instanceName: string;
  launching: boolean;
  log: LaunchLogLine[];
  onClear: () => void;
  onClose: () => void;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
  }, [log]);

  const scrollToTop = () => {
    scrollRef.current?.scrollTo({ top: 0, behavior: "smooth" });
  };

  const scrollToBottom = () => {
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
  };

  const copyLogs = async () => {
    const text = formatLaunchLog(log);
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      setCopied(false);
    }
  };

  return (
    <div className="fixed bottom-0 left-56 right-0 border-t border-border bg-card z-30 flex flex-col shadow-[0_-1px_0_0_var(--color-border)]">
      <div className="flex items-center justify-between gap-2 px-4 py-2 border-b border-border shrink-0">
        <div>
          <div className="text-sm font-medium">
            Console — {instanceName}
            {launching && <span className="text-muted-foreground ml-2">(running)</span>}
          </div>
          <div className="text-xs text-muted-foreground">{version} · Java stdout / stderr</div>
        </div>
        <div className="flex items-center gap-1">
          <Button variant="outline" size="sm" onClick={scrollToTop} disabled={log.length === 0}>
            Top
          </Button>
          <Button variant="outline" size="sm" onClick={scrollToBottom} disabled={log.length === 0}>
            Bottom
          </Button>
          <Button variant="outline" size="sm" onClick={copyLogs} disabled={log.length === 0}>
            {copied ? "Copied" : "Copy"}
          </Button>
          <Button variant="ghost" size="sm" onClick={onClear} disabled={log.length === 0 || launching}>
            Clear
          </Button>
          <Button variant="ghost" size="sm" onClick={onClose}>
            Close
          </Button>
        </div>
      </div>
      <ScrollArea ref={scrollRef} className="h-[240px] bg-black/60 p-3 font-mono text-xs">
        {log.length === 0 ? (
          <div className="text-muted-foreground">No log output yet. Launch the instance or wait for output.</div>
        ) : (
          log.map((entry, i) => (
            <div
              key={i}
              className={
                entry.stream === "stderr"
                  ? "text-red-400"
                  : entry.stream === "system"
                    ? "text-yellow-400"
                    : "text-green-400"
              }
            >
              {entry.line}
            </div>
          ))
        )}
      </ScrollArea>
    </div>
  );
}

// ponytail: one file, no router, no zustand

export default function App() {
  const [tab, setTab] = useState("instances");
  const [instances, setInstances] = useState<InstanceInfo[]>([]);
  const [dlProgress, setDlProgress] = useState<DlProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [editVersion, setEditVersion] = useState<string | null>(null);
  const [javaOptions, setJavaOptions] = useState<JavaInfo[]>([]);
  const [instanceLogs, setInstanceLogs] = useState<Record<string, LaunchLogLine[]>>({});
  const [launching, setLaunching] = useState<string | null>(null);
  const launchingRef = useRef<string | null>(null);
  const [consoleVersion, setConsoleVersion] = useState<string | null>(null);

  useEffect(() => {
    launchingRef.current = launching;
  }, [launching]);
  const [showNewInstance, setShowNewInstance] = useState(false);
  const [groupsState, setGroupsState] = useState<InstanceGroupsState>({ collapsed: {}, groups: [] });
  const [changeGroupVersion, setChangeGroupVersion] = useState<string | null>(null);
  const [lastUsedGroup, setLastUsedGroup] = useState("");

  const loadGroups = useCallback(() => {
    invoke<InstanceGroupsState>("get_instance_groups").then(setGroupsState).catch(() => {});
  }, []);

  useEffect(() => {
    loadInstances();
    loadGroups();
    invoke<JavaInfo[]>("detect_java").then(setJavaOptions).catch(() => {});
  }, [loadGroups]);

  useEffect(() => {
    const unlisten = listen<DlProgress>("dl-progress", (e) => {
      const p = e.payload;
      if (p.stage === "done") {
        setDlProgress(null);
        setShowNewInstance(false);
        loadInstances();
      } else {
        setDlProgress(p);
      }
    });
    return () => { unlisten.then((f) => f()); };
  }, []);

  useEffect(() => {
    const unlisten = listen<LaunchLogEvent>("launch-log", (e) => {
      const { version, stream, line } = e.payload;
      setInstanceLogs((prev) => ({
        ...prev,
        [version]: [...(prev[version] ?? []), { stream, line }],
      }));
    });
    return () => { unlisten.then((f) => f()); };
  }, []);

  const loadInstances = useCallback(() => {
    invoke<InstanceInfo[]>("get_instances")
      .then((list) => {
        setInstances(list);
        loadGroups();
      })
      .catch(() => {});
  }, [loadGroups]);

  const handleSetInstanceGroup = async (version: string, group: string) => {
    try {
      await invoke("set_instance_group", { version, group });
      setLastUsedGroup(group);
      loadInstances();
    } catch (e) {
      setError(`Change group failed: ${e}`);
    }
  };

  const handleRenameGroup = async (oldName: string, newName: string) => {
    try {
      await invoke("rename_group", { oldName, newName });
      loadInstances();
    } catch (e) {
      setError(`Rename group failed: ${e}`);
    }
  };

  const handleDeleteGroup = async (name: string) => {
    try {
      await invoke("delete_group", { name });
      loadInstances();
    } catch (e) {
      setError(`Delete group failed: ${e}`);
    }
  };

  const handleToggleGroupCollapsed = async (group: string, collapsed: boolean) => {
    setGroupsState((prev) => ({
      ...prev,
      collapsed: { ...prev.collapsed, [group]: collapsed },
    }));
    try {
      await invoke("set_group_collapsed", { group, collapsed });
    } catch (e) {
      setError(`Failed to save group state: ${e}`);
      loadGroups();
    }
  };

  const openConsole = async (version: string) => {
    setConsoleVersion(version);
    try {
      const persisted = await invoke<LaunchLogLine[]>("get_instance_console_log", { version });
      setInstanceLogs((prev) => {
        if (launchingRef.current === version && (prev[version]?.length ?? 0) > 0) {
          return prev;
        }
        return { ...prev, [version]: persisted };
      });
    } catch {
      // keep in-memory logs if file read fails
    }
  };

  const handleLaunch = async (version: string) => {
    if (launchingRef.current !== null) return;
    launchingRef.current = version;
    setError(null);
    setConsoleVersion(version);
    setLaunching(version);
    try {
      await invoke("launch_instance", { version });
    } catch (e) {
      setError(`Launch failed: ${e}`);
    } finally {
      launchingRef.current = null;
      setLaunching(null);
    }
  };

  const handleClearConsole = async (version: string) => {
    try {
      await invoke("clear_instance_console_log", { version });
      setInstanceLogs((prev) => ({ ...prev, [version]: [] }));
    } catch (e) {
      setError(`Clear console failed: ${e}`);
    }
  };

  const handleDelete = async (version: string) => {
    try {
      await invoke("delete_instance", { version });
      loadInstances();
    } catch (e) {
      setError(`Delete failed: ${e}`);
    }
  };

  const handleSaveSettings = async (version: string, settings: InstanceSettings) => {
    try {
      await invoke("save_settings", { version, settings });
      loadInstances();
    } catch (e) {
      setError(`Save failed: ${e}`);
    }
  };

  return (
    <div className="min-h-screen flex">
      {/* Sidebar */}
      <aside className="w-56 border-r border-border p-4 flex flex-col gap-2 shrink-0">
        <h1 className="text-lg font-semibold tracking-tight text-foreground mb-2">Industrialis</h1>

        <Button onClick={() => setShowNewInstance(true)} disabled={dlProgress !== null} className="w-full mb-4">
          + Add Instance
        </Button>

        <nav className="flex flex-col gap-1 flex-1">
          <button
            onClick={() => setTab("instances")}
            className={`text-left px-3 py-2 rounded text-sm transition-colors ${
              tab === "instances" ? "bg-muted text-foreground font-medium" : "hover:bg-muted/50 text-muted-foreground"
            }`}
          >
            Instances ({instances.length})
          </button>
          <button
            onClick={() => setTab("settings")}
            className={`text-left px-3 py-2 rounded text-sm transition-colors ${
              tab === "settings" ? "bg-muted text-foreground font-medium" : "hover:bg-muted/50 text-muted-foreground"
            }`}
          >
            Settings
          </button>
          <button
            onClick={() => setTab("accounts")}
            className={`text-left px-3 py-2 rounded text-sm transition-colors ${
              tab === "accounts" ? "bg-muted text-foreground font-medium" : "hover:bg-muted/50 text-muted-foreground"
            }`}
          >
            Accounts
          </button>
        </nav>

        <div className="mt-auto pt-4 border-t border-border flex justify-center">
          <ThemeSwitcher />
        </div>
      </aside>

      {/* Content */}
      <main className={`flex-1 p-6 overflow-auto ${consoleVersion ? "pb-[300px]" : ""}`}>
        {tab === "instances" && (<>
          {editVersion ? (
            <div>
              <Button variant="ghost" size="sm" onClick={() => setEditVersion(null)} className="mb-4">
                ← Back to instances
              </Button>
              <InstanceSettingsPanel
                version={editVersion}
                javaOptions={javaOptions}
                onSave={(v, s) => { handleSaveSettings(v, s); setEditVersion(null); }}
              />
            </div>
          ) : (
            <>
              {instances.length === 0 && (
                <p className="text-muted-foreground">No instances installed. Click "+ Add Instance" to get started.</p>
              )}
              <InstanceGroupList
                instances={instances}
                groupsState={groupsState}
                onToggleCollapsed={handleToggleGroupCollapsed}
                onRenameGroup={handleRenameGroup}
                onDeleteGroup={handleDeleteGroup}
                onLaunch={handleLaunch}
                onConsole={openConsole}
                onEdit={setEditVersion}
                onDelete={handleDelete}
                onRename={(version, name) => {
                  const inst = instances.find((i) => i.version === version);
                  if (inst) handleSaveSettings(version, { ...inst.settings, name });
                }}
                onChangeGroup={setChangeGroupVersion}
                disabled={launching !== null}
                consoleVersion={consoleVersion}
                launching={launching}
              />
            </>
          )}

          {showNewInstance && <NewInstanceDialog
            onClose={() => setShowNewInstance(false)}
            onInstall={async (version, javaType, group) => {
              setError(null);
              try {
                await invoke("download_install", { version, javaType, group: group || null });
                if (group) setLastUsedGroup(group);
              }
              catch (e) { setError(`Install failed: ${e}`); setDlProgress(null); }
            }}
            installedVersions={new Set(instances.map((i) => i.version))}
            existingGroups={groupsState.groups}
            initialGroup={lastUsedGroup}
          />}

          {changeGroupVersion && (
            <ChangeGroupDialog
              version={changeGroupVersion}
              currentGroup={instances.find((i) => i.version === changeGroupVersion)?.group ?? ""}
              existingGroups={groupsState.groups}
              onClose={() => setChangeGroupVersion(null)}
              onSave={(group) => {
                handleSetInstanceGroup(changeGroupVersion, group);
                setChangeGroupVersion(null);
              }}
            />
          )}
        </>)}

        {tab === "settings" && <SettingsTab javaOptions={javaOptions} />}
        {tab === "accounts" && <AccountsTab />}
      </main>

      {consoleVersion && (() => {
        const inst = instances.find((i) => i.version === consoleVersion);
        const name = inst?.settings.name || `GTNH ${consoleVersion}`;
        return (
          <LaunchConsole
            version={consoleVersion}
            instanceName={name}
            launching={launching === consoleVersion}
            log={instanceLogs[consoleVersion] ?? []}
            onClear={() => handleClearConsole(consoleVersion)}
            onClose={() => setConsoleVersion(null)}
          />
        );
      })()}

      {/* Error toast */}
      {error && (
        <div className="fixed bottom-4 right-4 bg-destructive text-destructive-foreground p-3 rounded shadow-lg max-w-sm z-50">
          <p className="text-sm">{error}</p>
          <Button size="sm" variant="ghost" className="mt-1" onClick={() => setError(null)}>Dismiss</Button>
        </div>
      )}

      {/* Download progress */}
      {dlProgress && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <Card className="w-80">
            <CardHeader>
              <CardTitle>
                {dlProgress.stage === "downloading" && "Downloading..."}
                {dlProgress.stage === "extracting" && "Extracting..."}
              </CardTitle>
              <CardDescription>Installing instance</CardDescription>
            </CardHeader>
            <CardContent>
              <Progress value={dlProgress.pct * 100} />
              <p className="text-sm text-muted-foreground mt-2">{(dlProgress.pct * 100).toFixed(0)}%</p>
            </CardContent>
          </Card>
        </div>
      )}
    </div>
  );
}

// ── Instance Groups ──

interface GroupSection {
  id: string;
  label: string;
  items: InstanceInfo[];
}

function buildGroupSections(
  instances: InstanceInfo[],
  groupNames: string[],
): GroupSection[] {
  const buckets = new Map<string, InstanceInfo[]>();
  for (const inst of instances) {
    const key = inst.group || "";
    if (!buckets.has(key)) buckets.set(key, []);
    buckets.get(key)!.push(inst);
  }

  const sections: GroupSection[] = [];
  const sortedNames = [...groupNames].sort((a, b) =>
    a.localeCompare(b, undefined, { sensitivity: "base" }),
  );

  for (const name of sortedNames) {
    const items = buckets.get(name);
    if (items && items.length > 0) {
      sections.push({ id: name, label: name, items });
      buckets.delete(name);
    }
  }

  for (const [key, items] of buckets) {
    if (key && items.length > 0) {
      sections.push({ id: key, label: key, items });
    }
  }

  const ungrouped = buckets.get("") ?? [];
  if (ungrouped.length > 0) {
    sections.push({ id: "", label: "Ungrouped", items: ungrouped });
  }

  return sections;
}

function InstanceGroupList({
  instances,
  groupsState,
  onToggleCollapsed,
  onRenameGroup,
  onDeleteGroup,
  onLaunch,
  onConsole,
  onEdit,
  onDelete,
  onRename,
  onChangeGroup,
  disabled,
  consoleVersion,
  launching,
}: {
  instances: InstanceInfo[];
  groupsState: InstanceGroupsState;
  onToggleCollapsed: (group: string, collapsed: boolean) => void;
  onRenameGroup: (oldName: string, newName: string) => void;
  onDeleteGroup: (name: string) => void;
  onLaunch: (version: string) => void;
  onConsole: (version: string) => void;
  onEdit: (version: string) => void;
  onDelete: (version: string) => void;
  onRename: (version: string, name: string) => void;
  onChangeGroup: (version: string) => void;
  disabled: boolean;
  consoleVersion: string | null;
  launching: string | null;
}) {
  const sections = useMemo(
    () => buildGroupSections(instances, groupsState.groups),
    [instances, groupsState.groups],
  );

  if (sections.length === 0) return null;

  return (
    <div className="space-y-6">
      {sections.map((section) => (
        <InstanceGroupSection
          key={section.id || "__ungrouped__"}
          section={section}
          collapsed={groupsState.collapsed[section.id] ?? false}
          onToggleCollapsed={(collapsed) => onToggleCollapsed(section.id, collapsed)}
          onRenameGroup={section.id ? onRenameGroup : undefined}
          onDeleteGroup={section.id ? onDeleteGroup : undefined}
          onLaunch={onLaunch}
          onConsole={onConsole}
          onEdit={onEdit}
          onDelete={onDelete}
          onRename={onRename}
          onChangeGroup={onChangeGroup}
          disabled={disabled}
          consoleVersion={consoleVersion}
          launching={launching}
        />
      ))}
    </div>
  );
}

function InstanceGroupSection({
  section,
  collapsed,
  onToggleCollapsed,
  onRenameGroup,
  onDeleteGroup,
  onLaunch,
  onConsole,
  onEdit,
  onDelete,
  onRename,
  onChangeGroup,
  disabled,
  consoleVersion,
  launching,
}: {
  section: GroupSection;
  collapsed: boolean;
  onToggleCollapsed: (collapsed: boolean) => void;
  onRenameGroup?: (oldName: string, newName: string) => void;
  onDeleteGroup?: (name: string) => void;
  onLaunch: (version: string) => void;
  onConsole: (version: string) => void;
  onEdit: (version: string) => void;
  onDelete: (version: string) => void;
  onRename: (version: string, name: string) => void;
  onChangeGroup: (version: string) => void;
  disabled: boolean;
  consoleVersion: string | null;
  launching: string | null;
}) {
  const [renaming, setRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState(section.label);

  useEffect(() => {
    setRenameValue(section.label);
  }, [section.label]);

  const commitRename = () => {
    setRenaming(false);
    const trimmed = renameValue.trim();
    if (!onRenameGroup || !section.id || !trimmed || trimmed === section.label) {
      setRenameValue(section.label);
      return;
    }
    onRenameGroup(section.id, trimmed);
  };

  const handleDelete = () => {
    if (!onDeleteGroup || !section.id) return;
    if (window.confirm(`Delete group "${section.label}"? Instances will be moved to Ungrouped.`)) {
      onDeleteGroup(section.id);
    }
  };

  return (
    <section>
      <div className="flex items-center gap-2 mb-3 group/header">
        <button
          type="button"
          onClick={() => onToggleCollapsed(!collapsed)}
          className="flex items-center gap-2 flex-1 min-w-0 text-left hover:text-foreground transition-colors"
          aria-expanded={!collapsed}
        >
          <span className="text-muted-foreground text-xs w-4 shrink-0">
            {collapsed ? "▶" : "▼"}
          </span>
          {renaming ? (
            <Input
              value={renameValue}
              onChange={(e) => setRenameValue(e.target.value)}
              onBlur={commitRename}
              onKeyDown={(e) => {
                if (e.key === "Enter") commitRename();
                if (e.key === "Escape") {
                  setRenaming(false);
                  setRenameValue(section.label);
                }
              }}
              onClick={(e) => e.stopPropagation()}
              className="h-7 text-sm max-w-xs"
              autoFocus
            />
          ) : (
            <h2 className="text-sm font-semibold tracking-tight truncate">{section.label}</h2>
          )}
          <Badge variant="secondary" className="shrink-0">
            {section.items.length}
          </Badge>
        </button>
        {section.id && onRenameGroup && onDeleteGroup && !renaming && (
          <div className="flex items-center gap-1 opacity-0 group-hover/header:opacity-100 transition-opacity">
            <Button
              variant="ghost"
              size="sm"
              className="h-7 px-2 text-xs"
              onClick={() => setRenaming(true)}
            >
              Rename
            </Button>
            <Button
              variant="ghost"
              size="sm"
              className="h-7 px-2 text-xs text-destructive hover:text-destructive"
              onClick={handleDelete}
            >
              Delete
            </Button>
          </div>
        )}
      </div>
      {!collapsed && (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {section.items.map((inst) => (
            <RenameableCard
              key={inst.version}
              inst={inst}
              onLaunch={() => onLaunch(inst.version)}
              onConsole={() => onConsole(inst.version)}
              onEdit={() => onEdit(inst.version)}
              onDelete={() => onDelete(inst.version)}
              onRename={(name) => onRename(inst.version, name)}
              onChangeGroup={() => onChangeGroup(inst.version)}
              disabled={disabled}
              consoleActive={consoleVersion === inst.version}
              running={launching === inst.version}
            />
          ))}
        </div>
      )}
    </section>
  );
}

function ChangeGroupDialog({
  version,
  currentGroup,
  existingGroups,
  onClose,
  onSave,
}: {
  version: string;
  currentGroup: string;
  existingGroups: string[];
  onClose: () => void;
  onSave: (group: string) => void;
}) {
  const [group, setGroup] = useState(currentGroup);
  const listId = "change-group-options";

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-40">
      <Card className="w-full max-w-sm">
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle>Change Group</CardTitle>
            <Button variant="ghost" size="sm" onClick={onClose}>✕</Button>
          </div>
          <CardDescription>{version}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div>
            <label className="text-sm font-medium">Group</label>
            <Input
              value={group}
              onChange={(e) => setGroup(e.target.value)}
              placeholder="No group"
              list={listId}
              onKeyDown={(e) => e.key === "Enter" && onSave(group.trim())}
              autoFocus
            />
            <datalist id={listId}>
              {existingGroups.map((g) => (
                <option key={g} value={g} />
              ))}
            </datalist>
            <p className="text-xs text-muted-foreground mt-1">
              Pick an existing group or type a new name. Leave empty for Ungrouped.
            </p>
          </div>
          <div className="flex gap-2">
            <Button className="flex-1" onClick={() => onSave(group.trim())}>Save</Button>
            <Button variant="outline" onClick={onClose}>Cancel</Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

function GroupPicker({
  value,
  onChange,
  existingGroups,
  id,
}: {
  value: string;
  onChange: (value: string) => void;
  existingGroups: string[];
  id: string;
}) {
  return (
    <div>
      <label className="text-sm font-medium">Group</label>
      <Input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder="No group"
        list={id}
      />
      <datalist id={id}>
        {existingGroups.map((g) => (
          <option key={g} value={g} />
        ))}
      </datalist>
    </div>
  );
}

// ── Renameable Instance Card ──

function RenameableCard({ inst, onLaunch, onConsole, onEdit, onDelete, onRename, onChangeGroup, disabled, consoleActive, running }: {
  inst: InstanceInfo;
  onLaunch: () => void;
  onConsole: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onRename: (name: string) => void;
  onChangeGroup: () => void;
  disabled: boolean;
  consoleActive: boolean;
  running: boolean;
}) {
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(inst.settings.name || `GTNH ${inst.version}`);

  useEffect(() => {
    setName(inst.settings.name || `GTNH ${inst.version}`);
  }, [inst.settings.name, inst.version]);

  const save = () => {
    setEditing(false);
    const trimmed = name.trim();
    if (trimmed && trimmed !== (inst.settings.name || `GTNH ${inst.version}`)) {
      onRename(trimmed);
    } else {
      setName(inst.settings.name || `GTNH ${inst.version}`);
    }
  };

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between gap-2">
          {editing ? (
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              onBlur={save}
              onKeyDown={(e) => e.key === "Enter" && save()}
              className="h-7 text-sm"
              autoFocus
            />
          ) : (
            <CardTitle
              className="text-lg cursor-pointer hover:text-foreground transition-colors"
              onClick={() => setEditing(true)}
              title="Click to rename"
            >
              {name}
            </CardTitle>
          )}
        </div>
        <CardDescription>{formatBytes(inst.size_bytes)}</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-wrap gap-2">
        <Button onClick={onLaunch} disabled={disabled}>
          {running ? "Launching..." : "Play"}
        </Button>
        <Button
          variant={consoleActive ? "default" : "outline"}
          onClick={onConsole}
        >
          Console
        </Button>
        <Button variant="secondary" onClick={onEdit}>Settings</Button>
        <Button variant="outline" onClick={onChangeGroup}>Group</Button>
        <Button variant="destructive" onClick={onDelete}>Delete</Button>
      </CardContent>
    </Card>
  );
}

// ── New Instance Dialog ──

function NewInstanceDialog({ onClose, onInstall, installedVersions, existingGroups, initialGroup }: {
  onClose: () => void;
  onInstall: (version: string, javaType: string, group: string) => void;
  installedVersions: Set<string>;
  existingGroups: string[];
  initialGroup: string;
}) {
  const [versions, setVersions] = useState<Record<string, GtnhVersion> | null>(null);
  const [filter, setFilter] = useState<"all" | "stable" | "beta">("all");
  const [sel, setSel] = useState<string | null>(null);
  const [javaType, setJavaType] = useState("java17+");
  const [group, setGroup] = useState(initialGroup);

  useEffect(() => {
    invoke<Record<string, GtnhVersion>>("get_versions").then(setVersions).catch(() => {});
  }, []);

  const sorted = versions
    ? Object.entries(versions).sort(([a], [b]) => b.localeCompare(a))
    : [];

  const filtered = filter === "stable"
    ? sorted.filter(([, v]) => v.title === "Stable release")
    : filter === "beta"
      ? sorted.filter(([, v]) => v.title !== "Stable release")
      : sorted;

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-40">
      <Card className="w-full max-w-lg max-h-[80vh] flex flex-col">
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle>New Instance</CardTitle>
            <Button variant="ghost" size="sm" onClick={onClose}>✕</Button>
          </div>
        </CardHeader>
        <CardContent className="flex-1 overflow-auto space-y-4">
          <div className="flex gap-2">
            <Select value={filter} onChange={(e) => setFilter(e.target.value as typeof filter)}>
              <option value="all">All versions</option>
              <option value="stable">Stable only</option>
              <option value="beta">Beta only</option>
            </Select>
            <Select value={javaType} onChange={(e) => setJavaType(e.target.value)}>
              <option value="java17+">Java 17+</option>
              <option value="java8">Java 8</option>
            </Select>
          </div>

          {!versions && <p className="text-muted-foreground text-sm">Loading versions...</p>}

          <div className="space-y-2">
            {filtered.map(([key, v]) => (
              <div
                key={key}
                className={`flex items-center justify-between p-3 rounded cursor-pointer border transition-colors ${
                  sel === key ? "border-foreground/30 bg-muted" : "border-border hover:bg-muted"
                }`}
                onClick={() => setSel(key)}
              >
                <div>
                  <div className="font-medium">{key}</div>
                  <div className="text-xs text-muted-foreground">
                    {v.releaseDate} · Max Java {v.maxJavaVersion}
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  <Badge variant={v.title === "Stable release" ? "success" : "warning"}>
                    {v.title === "Stable release" ? "Stable" : "Beta"}
                  </Badge>
                  {installedVersions.has(key) && (
                    <span className="text-xs text-muted-foreground">installed</span>
                  )}
                </div>
              </div>
            ))}
          </div>

          <GroupPicker
            value={group}
            onChange={setGroup}
            existingGroups={existingGroups}
            id="new-instance-group-options"
          />

          <Button className="w-full" disabled={!sel} onClick={() => sel && onInstall(sel, javaType, group.trim())}>
            Install {sel || ""}
          </Button>
        </CardContent>
      </Card>
    </div>
  );
}

// ── Instance Settings Panel ──

function InstanceSettingsPanel({ version, javaOptions, onSave }: {
  version: string;
  javaOptions: JavaInfo[];
  onSave: (version: string, settings: InstanceSettings) => void;
}) {
  const [settings, setSettings] = useState<InstanceSettings>({
    name: "", java_path: null, min_ram_mb: 4096, max_ram_mb: 6144,
    jvm_args: "", auth_mode: "offline", username: "Player",
  });

  useEffect(() => {
    invoke<InstanceSettings>("get_settings", { version }).then(setSettings).catch(() => {});
  }, [version]);

  const update = (p: Partial<InstanceSettings>) => setSettings((s) => ({ ...s, ...p }));

  return (
    <div className="space-y-4 max-w-lg">
      <div>
        <label className="text-sm font-medium">Instance Name</label>
        <Input value={settings.name}
          onChange={(e) => update({ name: e.target.value })}
          placeholder={`GTNH ${version}`} />
      </div>
      <div>
        <label className="text-sm font-medium">Java Path</label>
        <Select value={settings.java_path || ""} onChange={(e) => update({ java_path: e.target.value || null })}>
          <option value="">Auto-detect</option>
          {javaOptions.map((j) => (
            <option key={j.path} value={j.path}>Java {j.version} — {j.path}</option>
          ))}
        </Select>
      </div>
      <div className="grid grid-cols-2 gap-4">
        <div>
          <label className="text-sm font-medium">Min RAM (MB)</label>
          <Input type="number" value={settings.min_ram_mb}
            onChange={(e) => update({ min_ram_mb: parseInt(e.target.value) || 1024 })} />
        </div>
        <div>
          <label className="text-sm font-medium">Max RAM (MB)</label>
          <Input type="number" value={settings.max_ram_mb}
            onChange={(e) => update({ max_ram_mb: parseInt(e.target.value) || 2048 })} />
        </div>
      </div>
      <div>
        <label className="text-sm font-medium">Extra JVM Args</label>
        <Textarea value={settings.jvm_args}
          onChange={(e) => update({ jvm_args: e.target.value })}
          placeholder="-XX:+UseG1GC -XX:+UseCompactObjectHeaders" />
      </div>
      <div>
        <label className="text-sm font-medium">Auth Mode</label>
        <Select value={settings.auth_mode} onChange={(e) => update({ auth_mode: e.target.value })}>
          <option value="offline">Offline</option>
          <option value="microsoft">Microsoft</option>
        </Select>
      </div>
      <Button onClick={() => onSave(version, settings)}>Save</Button>
    </div>
  );
}

// ── Settings Tab ──

function SettingsTab({ javaOptions }: { javaOptions: JavaInfo[] }) {
  return (
    <div className="space-y-6 max-w-lg">
      <ThemeEditor />

      <Card>
        <CardHeader><CardTitle>Java Detection</CardTitle></CardHeader>
        <CardContent>
          <p className="text-sm text-muted-foreground mb-2">Detected Java installations:</p>
          {javaOptions.length === 0 && <p className="text-xs text-muted-foreground">None found</p>}
          <div className="space-y-1">
            {javaOptions.map((j) => (
              <div key={j.path} className="text-sm font-mono bg-muted p-2 rounded">
                <span className="text-foreground font-medium">Java {j.version}</span>
                <span className="text-muted-foreground ml-2 text-xs">{j.path}</span>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader><CardTitle>About</CardTitle></CardHeader>
        <CardContent className="text-sm text-muted-foreground space-y-1">
          <p>Industrialis Launcher v0.1.0</p>
          <p>GT New Horizons modpack manager built with Tauri.</p>
        </CardContent>
      </Card>
    </div>
  );
}

// ── Accounts Tab ──

function AccountsTab() {
  const [accounts, setAccounts] = useState<AccountInfo[]>([]);
  const [loggingIn, setLoggingIn] = useState(false);
  const [deviceCode, setDeviceCode] = useState<DeviceCodeInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = () => {
    invoke<AccountInfo[]>("get_accounts").then(setAccounts).catch(() => {});
  };
  useEffect(load, []);

  useEffect(() => {
    const unlisten = listen<DeviceCodeInfo>("auth-device-code", (e) => {
      setDeviceCode(e.payload);
    });
    return () => { unlisten.then((f) => f()); };
  }, []);

  const handleLogin = async () => {
    setLoggingIn(true);
    setError(null);
    setDeviceCode(null);
    try {
      await invoke<AccountInfo>("start_microsoft_login");
      load();
    } catch (e) {
      setError(`${e}`);
    }
    setLoggingIn(false);
    setDeviceCode(null);
  };

  const handleRemove = async (id: string) => {
    await invoke("remove_account", { id });
    load();
  };

  return (
    <div className="space-y-4 max-w-lg">
      <h2 className="text-lg font-semibold">Microsoft Accounts</h2>

      {accounts.length === 0 && (
        <p className="text-muted-foreground">No accounts linked.</p>
      )}

      {accounts.map((acc) => (
        <Card key={acc.id}>
          <CardHeader>
            <div className="flex items-center justify-between gap-3">
              <div className="flex items-center gap-3 min-w-0">
                {acc.skin_png_base64 && (
                  <img
                    src={`data:image/png;base64,${acc.skin_png_base64}`}
                    alt=""
                    className="w-8 h-8 rounded-sm shrink-0 image-pixelated"
                    style={{ imageRendering: "pixelated" }}
                  />
                )}
                <div className="min-w-0">
                  <CardTitle className="text-base truncate">
                    {acc.username || "(no Minecraft username)"}
                  </CardTitle>
                  {acc.uuid && (
                    <CardDescription className="font-mono text-xs truncate">{acc.uuid}</CardDescription>
                  )}
                </div>
              </div>
              <Button size="sm" variant="destructive" onClick={() => handleRemove(acc.id)}>Remove</Button>
            </div>
            {acc.owns_minecraft === false && (
              <p className="text-xs text-amber-500 mt-2">This account does not own Minecraft Java Edition.</p>
            )}
          </CardHeader>
        </Card>
      ))}

      {loggingIn && deviceCode && (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Device code login</CardTitle>
            <CardDescription>
              If the browser did not open, enter this code at{" "}
              <a className="text-foreground underline" href={deviceCode.verification_uri} target="_blank" rel="noreferrer">
                {deviceCode.verification_uri}
              </a>
            </CardDescription>
          </CardHeader>
          <CardContent>
            <p className="text-2xl font-mono tracking-widest">{deviceCode.user_code}</p>
            <p className="text-xs text-muted-foreground mt-2">{deviceCode.message}</p>
          </CardContent>
        </Card>
      )}

      <Button onClick={handleLogin} disabled={loggingIn} className="w-full">
        {loggingIn ? "Logging in..." : "Add Microsoft Account"}
      </Button>

      {error && <p className="text-destructive text-sm">{error}</p>}
    </div>
  );
}

import { useEffect, useState, useCallback, useRef } from "react";
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
}

interface LauncherSettingsData {
  microsoft_client_id: string;
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
    <div className="fixed bottom-0 left-56 right-0 border-t border-border bg-card z-30 flex flex-col shadow-[0_-4px_24px_rgba(0,0,0,0.4)]">
      <div className="flex items-center justify-between gap-2 px-4 py-2 border-b border-border shrink-0">
        <div>
          <div className="text-sm font-medium">
            Console — {instanceName}
            {launching && <span className="text-primary ml-2">(running)</span>}
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

  useEffect(() => {
    loadInstances();
    invoke<JavaInfo[]>("detect_java").then(setJavaOptions).catch(() => {});
  }, []);

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
    invoke<InstanceInfo[]>("get_instances").then(setInstances).catch(() => {});
  }, []);

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
        <h1 className="text-lg font-bold text-primary mb-2">Industrialis</h1>

        <Button onClick={() => setShowNewInstance(true)} disabled={dlProgress !== null} className="w-full mb-4">
          + Add Instance
        </Button>

        <nav className="flex flex-col gap-1">
          <button
            onClick={() => setTab("instances")}
            className={`text-left px-3 py-2 rounded text-sm transition-colors ${
              tab === "instances" ? "bg-primary/20 text-primary font-medium" : "hover:bg-muted text-muted-foreground"
            }`}
          >
            Instances ({instances.length})
          </button>
          <button
            onClick={() => setTab("settings")}
            className={`text-left px-3 py-2 rounded text-sm transition-colors ${
              tab === "settings" ? "bg-primary/20 text-primary font-medium" : "hover:bg-muted text-muted-foreground"
            }`}
          >
            Settings
          </button>
          <button
            onClick={() => setTab("accounts")}
            className={`text-left px-3 py-2 rounded text-sm transition-colors ${
              tab === "accounts" ? "bg-primary/20 text-primary font-medium" : "hover:bg-muted text-muted-foreground"
            }`}
          >
            Accounts
          </button>
        </nav>
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
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                {instances.map((inst) => (
                  <RenameableCard
                    key={inst.version}
                    inst={inst}
                    onLaunch={() => handleLaunch(inst.version)}
                    onConsole={() => openConsole(inst.version)}
                    onEdit={() => setEditVersion(inst.version)}
                    onDelete={() => handleDelete(inst.version)}
                    onRename={(name) => handleSaveSettings(inst.version, { ...inst.settings, name })}
                    disabled={launching !== null}
                    consoleActive={consoleVersion === inst.version}
                    running={launching === inst.version}
                  />
                ))}
              </div>
            </>
          )}

          {showNewInstance && <NewInstanceDialog
            onClose={() => setShowNewInstance(false)}
            onInstall={async (version, javaType) => {
              setError(null);
              try { await invoke("download_install", { version, javaType }); }
              catch (e) { setError(`Install failed: ${e}`); setDlProgress(null); }
            }}
            installedVersions={new Set(instances.map((i) => i.version))}
          />}
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

// ── Renameable Instance Card ──

function RenameableCard({ inst, onLaunch, onConsole, onEdit, onDelete, onRename, disabled, consoleActive, running }: {
  inst: InstanceInfo;
  onLaunch: () => void;
  onConsole: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onRename: (name: string) => void;
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
              className="text-lg cursor-pointer hover:text-primary transition-colors"
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
        <Button variant="destructive" onClick={onDelete}>Delete</Button>
      </CardContent>
    </Card>
  );
}

// ── New Instance Dialog ──

function NewInstanceDialog({ onClose, onInstall, installedVersions }: {
  onClose: () => void;
  onInstall: (version: string, javaType: string) => void;
  installedVersions: Set<string>;
}) {
  const [versions, setVersions] = useState<Record<string, GtnhVersion> | null>(null);
  const [filter, setFilter] = useState<"all" | "stable" | "beta">("all");
  const [sel, setSel] = useState<string | null>(null);
  const [javaType, setJavaType] = useState("java17+");

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
                  sel === key ? "border-primary bg-primary/10" : "border-border hover:bg-muted"
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

          <Button className="w-full" disabled={!sel} onClick={() => sel && onInstall(sel, javaType)}>
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
      <Card>
        <CardHeader><CardTitle>Java Detection</CardTitle></CardHeader>
        <CardContent>
          <p className="text-sm text-muted-foreground mb-2">Detected Java installations:</p>
          {javaOptions.length === 0 && <p className="text-xs text-muted-foreground">None found</p>}
          <div className="space-y-1">
            {javaOptions.map((j) => (
              <div key={j.path} className="text-sm font-mono bg-muted p-2 rounded">
                <span className="text-primary">Java {j.version}</span>
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
  const [tab, setTab] = useState<"accounts" | "setup">("accounts");
  const [accounts, setAccounts] = useState<AccountInfo[]>([]);
  const [settings, setSettings] = useState<LauncherSettingsData>({ microsoft_client_id: "" });
  const [loggingIn, setLoggingIn] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = () => {
    invoke<AccountInfo[]>("get_accounts").then(setAccounts).catch(() => {});
    invoke<LauncherSettingsData>("get_launcher_settings").then(setSettings).catch(() => {});
  };
  useEffect(load, []);

  const handleLogin = async () => {
    setLoggingIn(true);
    setError(null);
    try {
      await invoke<AccountInfo>("start_microsoft_login");
      load();
    } catch (e) {
      setError(`${e}`);
    }
    setLoggingIn(false);
  };

  const handleRemove = async (id: string) => {
    await invoke("remove_account", { id });
    load();
  };

  const handleSaveClientId = async () => {
    await invoke("save_launcher_settings", { settings });
  };

  if (tab === "setup") {
    return (
      <div className="space-y-4 max-w-lg">
        <Button variant="ghost" size="sm" onClick={() => setTab("accounts")} className="mb-2">← Back</Button>
        <Card>
          <CardHeader><CardTitle>Microsoft App Setup</CardTitle></CardHeader>
          <CardContent className="space-y-4 text-sm">
            <p className="text-muted-foreground">
              To use Microsoft login, you need to create an Azure app registration:
            </p>
            <ol className="space-y-2 text-muted-foreground list-decimal list-inside">
              <li>Go to <a className="text-primary underline" href="https://portal.azure.com/#view/Microsoft_AAD_RegisteredApps/ApplicationsListBlade" target="_blank">Azure portal → App registrations</a></li>
              <li>Register a new app (any name, supported account type: "Personal Microsoft accounts only")</li>
              <li>Add redirect URI: <code className="bg-muted px-1 rounded">http://localhost</code> (type: Web)</li>
              <li>Enable "Allow public client flows" → "Yes"</li>
              <li>Under "Certificates & secrets", note the Application (client) ID</li>
              <li>Paste the client ID below</li>
            </ol>
            <div>
              <label className="text-sm font-medium">Client ID</label>
              <Input value={settings.microsoft_client_id}
                onChange={(e) => setSettings({ microsoft_client_id: e.target.value })} />
            </div>
            <Button onClick={handleSaveClientId}>Save Client ID</Button>
          </CardContent>
        </Card>
        {error && <p className="text-destructive text-sm">{error}</p>}
      </div>
    );
  }

  return (
    <div className="space-y-4 max-w-lg">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold">Microsoft Accounts</h2>
        <Button variant="ghost" size="sm" onClick={() => setTab("setup")}>Setup</Button>
      </div>

      {!settings.microsoft_client_id && (
        <p className="text-sm text-muted-foreground">
          Configure your Microsoft client ID in Setup before logging in.
        </p>
      )}

      {accounts.length === 0 && (
        <p className="text-muted-foreground">No accounts linked.</p>
      )}

      {accounts.map((acc) => (
        <Card key={acc.id}>
          <CardHeader>
            <div className="flex items-center justify-between">
              <CardTitle className="text-base">{acc.username}</CardTitle>
              <Button size="sm" variant="destructive" onClick={() => handleRemove(acc.id)}>Remove</Button>
            </div>
            <CardDescription className="font-mono text-xs">{acc.uuid}</CardDescription>
          </CardHeader>
        </Card>
      ))}

      {settings.microsoft_client_id && (
        <Button onClick={handleLogin} disabled={loggingIn} className="w-full">
          {loggingIn ? "Logging in..." : "Add Microsoft Account"}
        </Button>
      )}

      {error && <p className="text-destructive text-sm">{error}</p>}
    </div>
  );
}

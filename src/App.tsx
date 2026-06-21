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

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

// ponytail: one file, no router, no zustand

export default function App() {
  const [tab, setTab] = useState("instances");
  const [instances, setInstances] = useState<InstanceInfo[]>([]);
  const [dlProgress, setDlProgress] = useState<DlProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [editVersion, setEditVersion] = useState<string | null>(null);
  const [javaOptions, setJavaOptions] = useState<JavaInfo[]>([]);
  const [launchLog, setLaunchLog] = useState<string[]>([]);
  const [launching, setLaunching] = useState<string | null>(null);
  const [showNewInstance, setShowNewInstance] = useState(false);
  const logEnd = useRef<HTMLDivElement>(null);

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

  useEffect(() => { logEnd.current?.scrollIntoView({ behavior: "smooth" }); }, [launchLog]);

  const loadInstances = useCallback(() => {
    invoke<InstanceInfo[]>("get_instances").then(setInstances).catch(() => {});
  }, []);

  const handleLaunch = async (version: string) => {
    setError(null);
    setLaunchLog([]);
    setLaunching(version);
    try {
      await invoke("launch_instance", { version });
    } catch (e) {
      setError(`Launch failed: ${e}`);
    }
    setLaunching(null);
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
      <main className="flex-1 p-6 overflow-auto">
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
                  <Card key={inst.version}>
                    <CardHeader>
                      <div className="flex items-center justify-between">
                        <CardTitle className="text-lg">GTNH {inst.version}</CardTitle>
                        {launching === inst.version && (
                          <span className="text-xs text-primary animate-pulse">Launching...</span>
                        )}
                      </div>
                      <CardDescription>{formatBytes(inst.size_bytes)}</CardDescription>
                    </CardHeader>
                    <CardContent className="flex gap-2">
                      <Button onClick={() => handleLaunch(inst.version)} disabled={launching !== null}>
                        Play
                      </Button>
                      <Button variant="secondary" onClick={() => setEditVersion(inst.version)}>
                        Settings
                      </Button>
                      <Button variant="destructive" onClick={() => handleDelete(inst.version)}>
                        Delete
                      </Button>
                    </CardContent>
                  </Card>
                ))}
              </div>

              {launching && launchLog.length > 0 && (
                <Card className="mt-4">
                  <CardHeader>
                    <CardTitle className="text-sm">Console — {launching}</CardTitle>
                  </CardHeader>
                  <CardContent>
                    <ScrollArea className="h-[200px] bg-black/50 rounded p-3 font-mono text-xs">
                      {launchLog.map((line, i) => (
                        <div key={i} className="text-green-400">{line}</div>
                      ))}
                      <div ref={logEnd} />
                    </ScrollArea>
                  </CardContent>
                </Card>
              )}
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
    java_path: null, min_ram_mb: 4096, max_ram_mb: 6144,
    jvm_args: "", auth_mode: "offline", username: "Player",
  });

  useEffect(() => {
    invoke<InstanceSettings>("get_settings", { version }).then(setSettings).catch(() => {});
  }, [version]);

  const update = (p: Partial<InstanceSettings>) => setSettings((s) => ({ ...s, ...p }));

  return (
    <div className="space-y-4 max-w-lg">
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
  const [mode, setMode] = useState("offline");
  const [username, setUsername] = useState("Player");

  return (
    <div className="space-y-6 max-w-lg">
      <Card>
        <CardHeader><CardTitle>Authentication</CardTitle></CardHeader>
        <CardContent className="space-y-4">
          <div>
            <label className="text-sm font-medium">Auth Mode</label>
            <Select value={mode} onChange={(e) => setMode(e.target.value)} className="mt-1">
              <option value="offline">Offline</option>
              <option value="microsoft" disabled>Microsoft (coming soon)</option>
            </Select>
          </div>
          <div>
            <label className="text-sm font-medium">Default Username</label>
            <Input value={username} onChange={(e) => setUsername(e.target.value)} className="mt-1" />
          </div>
          <p className="text-xs text-muted-foreground">
            Each instance can override these in its own settings.
          </p>
        </CardContent>
      </Card>
    </div>
  );
}

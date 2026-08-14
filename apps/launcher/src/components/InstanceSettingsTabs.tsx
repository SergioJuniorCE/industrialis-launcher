import { useRef, useState, type ReactNode } from "react";

import { Button } from "./ui/button";
import { Label } from "./ui/label";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "./ui/card";
import { Checkbox } from "./ui/checkbox";
import { Input } from "./ui/input";
import { Select } from "./ui/select";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "./ui/tabs";
import { Textarea } from "./ui/textarea";
import { formatPlayTime, type InstanceSettings } from "../lib/instance-settings";

export interface AccountOption {
  id: string;
  username: string;
}

function SettingsSection({
  title,
  description,
  overrideLabel,
  override,
  onOverrideChange,
  children,
}: {
  title: string;
  description?: string;
  overrideLabel?: string;
  override?: boolean;
  onOverrideChange?: (value: boolean) => void;
  children: ReactNode;
}) {
  const enabled = override === undefined || override;
  return (
    <Card className="border-border/80 shadow-none">
      <CardHeader className="pb-2">
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0">
            <CardTitle className="text-sm">{title}</CardTitle>
            {description ? <CardDescription className="mt-0.5 text-xs">{description}</CardDescription> : null}
          </div>
          {onOverrideChange ? (
            <Checkbox
              checked={override}
              onChange={(e) => onOverrideChange(e.target.checked)}
              label={overrideLabel ?? "Override"}
              className="shrink-0 text-xs"
            />
          ) : null}
        </div>
      </CardHeader>
      <CardContent inert={!enabled} className={enabled ? "space-y-2" : "space-y-2 opacity-50"}>
        {children}
      </CardContent>
    </Card>
  );
}

function FieldLabel({ children, hint }: { children: ReactNode; hint?: string }) {
  return (
    <div>
      <Label className="text-xs">{children}</Label>
      {hint ? <p className="text-[11px] text-muted-foreground mt-0.5">{hint}</p> : null}
    </div>
  );
}

type EnvRow = {
  key: string;
  value: string;
  persistedKey: string | null;
};

function envRowsToRecord(rows: readonly EnvRow[]): Record<string, string> {
  const result: Record<string, string> = {};
  for (const row of rows) {
    const trimmed = row.key.trim();
    if (trimmed) result[trimmed] = row.value;
  }
  return result;
}

function envRowsFromValue(value: Record<string, string>): EnvRow[] {
  const entries = Object.entries(value);
  return entries.length > 0 ? entries.map(([key, rowValue]) => ({ key, value: rowValue, persistedKey: key })) : [{ key: "", value: "", persistedKey: null }];
}

function EnvVarEditor({ value, onChange }: { value: Record<string, string>; onChange: (next: Record<string, string>) => void }) {
  const persistedRows = envRowsFromValue(value);
  const [draftRows, setDraftRows] = useState<EnvRow[] | null>(null);
  const rows = draftRows ?? persistedRows;
  const rowKeys = useRef<string[]>([]);
  const nextRowKey = useRef(0);

  const getStableRowKey = (index: number): string => {
    const existing = rowKeys.current[index];
    if (existing) return existing;
    const key = `env-row-${nextRowKey.current}`;
    nextRowKey.current += 1;
    rowKeys.current[index] = key;
    return key;
  };

  const updateRow = (index: number, key: string, val: string) => {
    const existing = rows[index];
    if (!existing) return;
    const trimmedKey = key.trim();
    const next = rows.map((row, rowIndex) => (rowIndex === index ? { ...row, key, value: val, persistedKey: trimmedKey || row.persistedKey } : row));
    setDraftRows(next);
    if (trimmedKey) onChange(envRowsToRecord(next));
  };

  const addRow = () => setDraftRows([...rows, { key: "", value: "", persistedKey: null }]);

  const removeRow = (index: number) => {
    const removed = rows[index];
    if (!removed) return;
    const next = rows.filter((_, i) => i !== index);
    const nextRows: EnvRow[] = next.length > 0 ? next : [{ key: "", value: "", persistedKey: null }];
    rowKeys.current.splice(index, 1);
    setDraftRows(nextRows);
    if (removed.persistedKey) onChange(envRowsToRecord(nextRows));
  };

  return (
    <div className="space-y-2">
      {rows.map((row, index) => (
        <div key={getStableRowKey(index)} className="flex gap-2">
          <Input value={row.key} onChange={(e) => updateRow(index, e.target.value, row.value)} placeholder="VAR_NAME" className="font-mono text-xs" />
          <Input value={row.value} onChange={(e) => updateRow(index, row.key, e.target.value)} placeholder="value" className="font-mono text-xs flex-1" />
          <Button type="button" variant="ghost" size="sm" onClick={() => removeRow(index)}>
            ×
          </Button>
        </div>
      ))}
      <Button type="button" variant="outline" size="sm" onClick={addRow}>
        Add variable
      </Button>
    </div>
  );
}

function AdvancedSettingsTabs({
  instanceId,
  settings,
  update,
}: {
  instanceId: string;
  settings: InstanceSettings;
  update: (patch: Partial<InstanceSettings>) => void;
}) {
  return (
    <>
      <TabsContent value="commands" className="space-y-2 mt-2">
        <SettingsSection
          title="Custom commands"
          description="Pre-launch runs before the game starts; post-exit runs after it closes. Wrapper wraps the Java command."
          override={settings.override_commands}
          onOverrideChange={(value) => update({ override_commands: value })}
        >
          <FieldLabel>Pre-launch command</FieldLabel>
          <Input
            className="mt-1 font-mono text-xs"
            value={settings.pre_launch_command}
            onChange={(event) => update({ pre_launch_command: event.target.value })}
          />
          <FieldLabel>Wrapper command</FieldLabel>
          <Input
            className="mt-1 font-mono text-xs"
            value={settings.wrapper_command}
            onChange={(event) => update({ wrapper_command: event.target.value })}
            placeholder="e.g. optirun"
          />
          <FieldLabel>Post-exit command</FieldLabel>
          <Input
            className="mt-1 font-mono text-xs"
            value={settings.post_exit_command}
            onChange={(event) => update({ post_exit_command: event.target.value })}
          />
          <p className="text-xs text-muted-foreground leading-relaxed">Available variables: $INST_NAME, $INST_ID, $INST_DIR, $INST_MC_DIR, $INST_JAVA</p>
        </SettingsSection>
      </TabsContent>

      <TabsContent value="environment" className="space-y-2 mt-2">
        <SettingsSection title="Environment variables" override={settings.override_env} onOverrideChange={(value) => update({ override_env: value })}>
          <EnvVarEditor key={instanceId} value={settings.env_vars} onChange={(env_vars) => update({ env_vars })} />
        </SettingsSection>
      </TabsContent>
    </>
  );
}

function LauncherSettingsLink({ onOpen }: { onOpen?: () => void }) {
  return (
    <Button
      type="button"
      variant="outline"
      onClick={onOpen}
      className="h-auto w-full flex-col items-start gap-0.5 bg-muted/30 px-3 py-2 text-left font-normal hover:bg-muted/50"
    >
      <span className="font-medium text-xs">Open launcher settings</span>
      <span className="text-[11px] text-muted-foreground font-normal">
        Instance settings below override defaults only when each section&apos;s override box is checked.
      </span>
    </Button>
  );
}

function GeneralSettingsTab({
  settings,
  packVersion,
  accounts,
  update,
}: {
  settings: InstanceSettings;
  packVersion: string;
  accounts: AccountOption[];
  update: (patch: Partial<InstanceSettings>) => void;
}) {
  return (
    <TabsContent value="general" className="space-y-2 mt-2">
      <Card className="shadow-none">
        <CardHeader className="pb-2">
          <CardTitle className="text-sm">Instance</CardTitle>
        </CardHeader>
        <CardContent>
          <FieldLabel>Display name</FieldLabel>
          <Input className="mt-1" value={settings.name} onChange={(e) => update({ name: e.target.value })} placeholder={`GTNH ${packVersion}`} />
        </CardContent>
      </Card>

      <SettingsSection title="Game window" override={settings.override_window} onOverrideChange={(v) => update({ override_window: v })}>
        <Checkbox checked={settings.launch_maximized} onChange={(e) => update({ launch_maximized: e.target.checked })} label="Start Minecraft maximized" />
        {!settings.launch_maximized && (
          <div className="flex items-center gap-2 flex-wrap">
            <FieldLabel hint="Passed as --width / --height at launch">Window size</FieldLabel>
            <Input
              type="number"
              className="w-24"
              value={settings.window_width}
              onChange={(e) => update({ window_width: parseInt(e.target.value, 10) || 854 })}
            />
            <span className="text-muted-foreground">×</span>
            <Input
              type="number"
              className="w-24"
              value={settings.window_height}
              onChange={(e) => update({ window_height: parseInt(e.target.value, 10) || 480 })}
            />
            <span className="text-xs text-muted-foreground">pixels</span>
          </div>
        )}
        <Checkbox
          checked={settings.close_after_launch}
          onChange={(e) => update({ close_after_launch: e.target.checked })}
          label="When the game window opens, hide the launcher"
        />
        <Checkbox
          checked={settings.quit_after_game_stop}
          onChange={(e) => update({ quit_after_game_stop: e.target.checked })}
          label="When the game window closes, quit the launcher"
        />
      </SettingsSection>

      <SettingsSection
        title="Console / logs"
        description="Controls the in-launcher log panel."
        override={settings.override_console}
        onOverrideChange={(v) => update({ override_console: v })}
      >
        <Checkbox
          checked={settings.show_console_on_launch}
          onChange={(e) => update({ show_console_on_launch: e.target.checked })}
          label="When the game is launched, show the log panel"
        />
        <Checkbox
          checked={settings.show_console_on_error}
          onChange={(e) => update({ show_console_on_error: e.target.checked })}
          label="When the game crashes, show the log panel"
        />
        <Checkbox
          checked={settings.auto_close_console}
          onChange={(e) => update({ auto_close_console: e.target.checked })}
          label="When the game quits, hide the log panel"
        />
      </SettingsSection>

      <SettingsSection title="Play time" override={settings.override_game_time} onOverrideChange={(v) => update({ override_game_time: v })}>
        <Checkbox
          checked={settings.show_game_time}
          onChange={(e) => update({ show_game_time: e.target.checked })}
          label="Show time spent playing this instance"
        />
        <Checkbox
          checked={settings.record_game_time}
          onChange={(e) => update({ record_game_time: e.target.checked })}
          label="Record time spent playing this instance"
        />
        {settings.total_play_seconds > 0 && <p className="text-xs text-muted-foreground">Total recorded: {formatPlayTime(settings.total_play_seconds)}</p>}
      </SettingsSection>

      <SettingsSection title="Override default account" override={settings.override_account} onOverrideChange={(v) => update({ override_account: v })}>
        <FieldLabel hint="Leave empty to use the launcher default account for every launch.">Account</FieldLabel>
        <Select className="mt-1" value={settings.account_id ?? ""} onChange={(e) => update({ account_id: e.target.value || null })}>
          <option value="">Use launcher default account</option>
          {accounts.map((a) => (
            <option key={a.id} value={a.id}>
              {a.username || a.id}
            </option>
          ))}
        </Select>
      </SettingsSection>

      <SettingsSection
        title="Auto-join server"
        override={settings.join_server_on_launch}
        onOverrideChange={(v) => update({ join_server_on_launch: v })}
        overrideLabel="Enable"
      >
        <FieldLabel hint="Connects on launch when supported (1.7.10: passes --server).">Server address</FieldLabel>
        <Input
          className="mt-1 font-mono text-sm"
          value={settings.join_server_address}
          onChange={(e) => update({ join_server_address: e.target.value })}
          placeholder="play.example.com"
        />
      </SettingsSection>
    </TabsContent>
  );
}

function JavaSettingsTab({
  settings,
  javaRefreshing,
  javaTestResult,
  testingJava,
  update,
  onDetectJava,
  onBrowseJava,
  onTestJava,
}: {
  settings: InstanceSettings;
  javaRefreshing: boolean;
  javaTestResult: string | null;
  testingJava: boolean;
  update: (patch: Partial<InstanceSettings>) => void;
  onDetectJava: () => void;
  onBrowseJava: () => void;
  onTestJava: () => void;
}) {
  return (
    <TabsContent value="java" className="space-y-2 mt-2">
      <SettingsSection title="Java installation" override={settings.override_java_location} onOverrideChange={(v) => update({ override_java_location: v })}>
        <FieldLabel>Java executable</FieldLabel>
        <Input
          className="mt-1 font-mono text-xs"
          value={settings.java_path ?? ""}
          onChange={(e) => update({ java_path: e.target.value || null })}
          placeholder="Uses launcher default when override is off"
        />
        <div className="flex flex-wrap gap-2">
          <Button type="button" variant="outline" size="sm" disabled={javaRefreshing} onClick={onDetectJava}>
            {javaRefreshing ? "Scanning..." : "Detect"}
          </Button>
          <Button type="button" variant="outline" size="sm" onClick={onBrowseJava}>
            Browse
          </Button>
          <Button type="button" variant="outline" size="sm" onClick={onTestJava} disabled={testingJava}>
            {testingJava ? "Testing…" : "Test settings"}
          </Button>
        </div>
        {javaTestResult ? <pre className="text-xs bg-muted rounded p-2 whitespace-pre-wrap font-mono">{javaTestResult}</pre> : null}
        <Checkbox checked={settings.skip_java_compat} onChange={(e) => update({ skip_java_compat: e.target.checked })} label="Skip Java compatibility checks" />
      </SettingsSection>

      <SettingsSection title="Memory" override={settings.override_memory} onOverrideChange={(v) => update({ override_memory: v })}>
        <div className="grid gap-3 sm:grid-cols-2">
          <div>
            <FieldLabel hint="-Xms">Minimum memory (MiB)</FieldLabel>
            <Input type="number" className="mt-1" value={settings.min_ram_mb} onChange={(e) => update({ min_ram_mb: parseInt(e.target.value, 10) || 512 })} />
          </div>
          <div>
            <FieldLabel hint="-Xmx">Maximum memory (MiB)</FieldLabel>
            <Input type="number" className="mt-1" value={settings.max_ram_mb} onChange={(e) => update({ max_ram_mb: parseInt(e.target.value, 10) || 1024 })} />
          </div>
        </div>
        <div>
          <FieldLabel hint="-XX:PermSize / MaxPermSize (Java 7 / 1.7.10)">PermGen (MiB)</FieldLabel>
          <Input
            type="number"
            className="mt-1 w-32"
            value={settings.perm_gen_mb}
            onChange={(e) => update({ perm_gen_mb: parseInt(e.target.value, 10) || 0 })}
          />
        </div>
      </SettingsSection>

      <SettingsSection title="Java arguments" override={settings.override_java_args} onOverrideChange={(v) => update({ override_java_args: v })}>
        <Textarea
          value={settings.jvm_args}
          onChange={(e) => update({ jvm_args: e.target.value })}
          placeholder="-XX:+UseG1GC"
          className="font-mono text-xs min-h-28"
        />
      </SettingsSection>
    </TabsContent>
  );
}

export function InstanceSettingsTabs({
  instanceId,
  settings,
  packVersion,
  settingsTab,
  accounts,
  javaRefreshing,
  javaTestResult,
  testingJava,
  onSettingsTabChange,
  onUpdate,
  onDetectJava,
  onBrowseJava,
  onTestJava,
  onOpenLauncherSettings,
}: {
  instanceId: string;
  settings: InstanceSettings;
  packVersion: string;
  settingsTab: string;
  accounts: AccountOption[];
  javaRefreshing: boolean;
  javaTestResult: string | null;
  testingJava: boolean;
  onSettingsTabChange: (value: string) => void;
  onUpdate: (patch: Partial<InstanceSettings>) => void;
  onDetectJava: () => void;
  onBrowseJava: () => void;
  onTestJava: () => void;
  onOpenLauncherSettings?: () => void;
}) {
  return (
    <div className="flex flex-col gap-2 max-w-2xl pb-2">
      <LauncherSettingsLink onOpen={onOpenLauncherSettings} />
      <Tabs value={settingsTab} onValueChange={onSettingsTabChange} className="w-full">
        <TabsList className="flex flex-wrap h-auto gap-0.5">
          <TabsTrigger value="general">General</TabsTrigger>
          <TabsTrigger value="java">Java</TabsTrigger>
          <TabsTrigger value="commands">Custom Commands</TabsTrigger>
          <TabsTrigger value="environment">Environment</TabsTrigger>
        </TabsList>
        <GeneralSettingsTab settings={settings} packVersion={packVersion} accounts={accounts} update={onUpdate} />
        <JavaSettingsTab
          settings={settings}
          javaRefreshing={javaRefreshing}
          javaTestResult={javaTestResult}
          testingJava={testingJava}
          update={onUpdate}
          onDetectJava={onDetectJava}
          onBrowseJava={onBrowseJava}
          onTestJava={onTestJava}
        />
        <AdvancedSettingsTabs instanceId={instanceId} settings={settings} update={onUpdate} />
      </Tabs>
    </div>
  );
}

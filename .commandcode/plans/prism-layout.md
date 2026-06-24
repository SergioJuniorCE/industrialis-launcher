# Prism-style 3-panel layout for Industrialis Launcher

**Goal:** Reskin `src/App.tsx` into Prism Launcher's layout (top toolbar + instance list / tabbed details panel + bottom status bar), keeping the existing monochrome theme and all Rust handlers. Ponytail: reuse every handler, only reorganize JSX + swap card grid for selectable rows.

Single file changed: `src/App.tsx`. No new files, no deps (lucide-react + shadcn `Tabs` already installed).

## What stays
All Rust `invoke` calls, `listen` subscriptions, `NewInstanceDialog`, `ChangeGroupDialog`, `GroupPicker`, `InstanceSettingsPanel`, `SettingsTab`, `AccountsTab`, `buildGroupSections`, `formatBytes`, `formatLaunchLog`, theme system (`App.css` untouched).

## Edits (in order)

### 1. Imports — add two lines
- After `import { listen } ...;` add lucide icons:
  `import { Plus, Settings, Users, Boxes, Play, Trash2, FolderInput, Info, Terminal, SlidersHorizontal } from "lucide-react";`
- After `import { ScrollArea } ...;` add:
  `import { Tabs, TabsList, TabsTrigger, TabsContent } from "./components/ui/tabs";`

### 2. State swap (in `App`)
Replace `editVersion` + `consoleVersion` state with:
```ts
const [selectedVersion, setSelectedVersion] = useState<string | null>(null);
const [detailTab, setDetailTab] = useState("info");
```
Remove `consoleVersion`. (`launching`/`launchingRef`/`instanceLogs` stay.)

### 3. `openConsole` -> `loadLogs` + selection effect
Replace the `openConsole` fn with `loadLogs(version)` (same body, minus `setConsoleVersion`). Add:
```ts
useEffect(() => { if (selectedVersion) loadLogs(selectedVersion); }, [selectedVersion, loadLogs]);
```

### 4. `handleLaunch`
Drop `setConsoleVersion(version);`; add `setDetailTab("logs");` so launching jumps to the Logs tab.

### 5. Rewrite `App` return
Before `return`, add: `const sel = instances.find((i) => i.version === selectedVersion) ?? null;`

New layout (replaces sidebar + main + fixed console), `<div className="h-screen flex flex-col overflow-hidden">`:
- **Toolbar** `<header className="h-12 ... bg-card">`: "Industrialis" label, `Add Instance` (Plus) button, divider, Instances/Settings/Accounts toggles (Boxes/Settings/Users, `variant=secondary` when active), `ThemeSwitcher` at right (`ml-auto`).
- **Instances tab body** `<div className="flex-1 flex overflow-hidden">`:
  - **List panel** `<div className="w-1/2 max-w-xl border-r overflow-auto flex flex-col">`: empty-state text or `<InstanceGroupList .../>` with new props (`selectedVersion`, `onSelect={setSelectedVersion}`, `launching`; drop onLaunch/onConsole/onEdit/onDelete/onRename/onChangeGroup/disabled/consoleVersion).
  - **Details panel** `<div className="flex-1 flex flex-col overflow-hidden">`: if `sel`:
    - Header: letter avatar + name + `version . size . group`.
    - `<Tabs value={detailTab} onValueChange={setDetailTab} className="flex-1 flex flex-col overflow-hidden">` triggers Info (Info) / Settings (SlidersHorizontal) / Logs (Terminal).
      - Info: `<InfoRow>` list (Version, Size, Group, Java, RAM, Auth, Username).
      - Settings: `<InstanceSettingsPanel version={selectedVersion!} javaOptions={javaOptions} onSave={(v,s)=>handleSaveSettings(v,s)} />` (no back button).
      - Logs: `<LogView log={instanceLogs[selectedVersion!] ?? []} onClear={()=>handleClearConsole(selectedVersion!)} disableClear={launching===selectedVersion} />`.
    - Action bar `<div className="shrink-0 border-t p-3 flex gap-2">`: big `Launch` (Play, disabled when `launching!==null`, label "Launching..."/"Busy"/"Launch"), `FolderInput` icon -> `setChangeGroupVersion(selectedVersion)`, `Trash2` icon -> `handleDelete(selectedVersion!)`.
    - else: centered "Select an instance to view details."
- **Non-instances tabs**: `<main className="flex-1 overflow-auto p-6">` rendering `SettingsTab`/`AccountsTab` (unchanged).
- **Status bar** `<footer className="h-6 ... bg-card text-xs text-muted-foreground">`: instance count, selected name, "Launching X...", install %.
- Keep `NewInstanceDialog`, `ChangeGroupDialog`, error toast (move to `bottom-8`), download overlay unchanged.

### 6. Delete `LaunchConsole` component
Dead after the rewrite (logs now live in the Logs tab via `LogView`). Keep `formatLaunchLog` (used by `LogView`).

### 7. Convert instance display components
- `InstanceGroupList`: drop action props; take `selectedVersion`, `onSelect`, `launching`. Sections in a plain `<div>`.
- `InstanceGroupSection`: simpler header (`sticky top-0 bg-background z-10`, uppercase muted label, count badge, hover rename/delete). Render `InstanceRow`s (no grid) when expanded.
- Replace `RenameableCard` with `InstanceRow`: button row, letter avatar, name + `version . size`, green pulse dot when running, `bg-muted` when selected.
- Add `InfoRow` (label/value row) and `LogView` (ref + `useEffect` auto-scroll on `[log]`, Copy via `formatLaunchLog`, Clear, colored stdout/stderr/system lines in a `ScrollArea`).

Rename still available via the Settings tab's Instance Name field, so the inline card rename is dropped (ponytail: deletion).

## Verification
1. `npx tsc --noEmit` (strict + noUnusedLocals must pass -> no dangling `consoleVersion`/`editVersion`/`LaunchConsole` refs).
2. `npm run build` (Vite) succeeds.
3. `npm run tauri dev`: toolbar switches Instances/Settings/Accounts; clicking a row selects it and shows Info/Settings/Logs tabs; Launch streams logs into the Logs tab; status bar updates; Add Instance / Change Group / Delete work; theme switcher + editor intact; dark/light still monochrome.

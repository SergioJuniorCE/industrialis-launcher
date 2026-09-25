import { InstanceMinecraftEditor } from "./InstanceMinecraftEditor";

export function MinecraftEditorWindow({ instanceId, initialPath }: { instanceId: string; initialPath?: string }) {
  return (
    <main className="app-shell flex h-screen min-h-0 flex-col overflow-hidden">
      <header className="flex h-12 shrink-0 items-center border-b border-border bg-card px-4">
        <div className="min-w-0">
          <h1 className="truncate text-sm font-semibold">Minecraft file editor</h1>
          <p className="truncate text-xs text-muted-foreground">{instanceId} · .minecraft</p>
        </div>
      </header>
      <div className="min-h-0 flex-1 p-3">
        <InstanceMinecraftEditor instanceId={instanceId} initialPath={initialPath} standalone />
      </div>
    </main>
  );
}

import { Play, Settings, Terminal, Users } from "lucide-react";

export function LauncherPreview() {
  return (
    <div className="relative mx-auto w-full max-w-2xl">
      <div
        aria-hidden
        className="pointer-events-none absolute -inset-4 rounded-2xl bg-accent/10 blur-3xl"
      />
      <div className="relative overflow-hidden rounded-xl border border-border bg-card shadow-2xl shadow-black/40">
        <div className="flex items-center gap-2 border-b border-border bg-muted/50 px-3 py-2">
          <span className="size-2.5 rounded-full bg-red-500/80" />
          <span className="size-2.5 rounded-full bg-amber-500/80" />
          <span className="size-2.5 rounded-full bg-emerald-500/80" />
          <span className="ml-2 font-mono text-[10px] text-muted-foreground">
            Industrialis Launcher
          </span>
        </div>

        <div className="flex items-center gap-1 border-b border-border px-2 py-1.5">
          {[
            { icon: Play, label: "Instances", active: true },
            { icon: Settings, label: "Settings", active: false },
            { icon: Users, label: "Accounts", active: false },
          ].map(({ icon: Icon, label, active }) => (
            <span
              key={label}
              className={`flex items-center gap-1 rounded px-2 py-1 text-[10px] ${
                active
                  ? "bg-background text-foreground"
                  : "text-muted-foreground"
              }`}
            >
              <Icon className="size-3" />
              {label}
            </span>
          ))}
        </div>

        <div className="grid min-h-[220px] grid-cols-[140px_1fr] text-[10px]">
          <aside className="border-r border-border bg-muted/30 p-2">
            <p className="mb-2 font-mono text-[9px] uppercase tracking-wider text-muted-foreground">
              Installed
            </p>
            <div className="space-y-1">
              <div className="rounded border border-accent/40 bg-background px-2 py-1.5">
                <p className="font-medium">GTNH 2.7.4</p>
                <p className="text-muted-foreground">Java 17 · 6.2 GB</p>
              </div>
              <div className="rounded border border-transparent px-2 py-1.5 text-muted-foreground">
                <p>GTNH 2.6.1</p>
                <p className="text-[9px]">Java 8 · 4.0 GB</p>
              </div>
            </div>
          </aside>

          <section className="flex flex-col p-3">
            <div className="mb-3 flex items-start justify-between gap-2">
              <div>
                <h3 className="text-sm font-semibold">GTNH 2.7.4</h3>
                <p className="text-muted-foreground">Stable · Max Java 21</p>
              </div>
              <span className="flex items-center gap-1 rounded bg-emerald-500/15 px-2 py-1 text-[9px] font-medium text-emerald-400">
                <Play className="size-2.5 fill-current" />
                Play
              </span>
            </div>
            <div className="flex flex-1 flex-col rounded border border-border bg-background p-2 font-mono">
              <div className="mb-1 flex items-center gap-1 text-[9px] text-muted-foreground">
                <Terminal className="size-3" />
                Console
              </div>
              <p className="text-[9px] leading-relaxed text-muted-foreground">
                <span className="text-emerald-400/90">[system]</span> Java 17.0.12
                detected
              </p>
              <p className="text-[9px] leading-relaxed text-muted-foreground">
                <span className="text-emerald-400/90">[stdout]</span> Loading
                FML...
              </p>
              <p className="text-[9px] leading-relaxed text-muted-foreground">
                <span className="text-emerald-400/90">[stdout]</span>{" "}
                Constructing mods...
              </p>
            </div>
          </section>
        </div>

        <div className="border-t border-border px-3 py-1.5 font-mono text-[9px] text-muted-foreground">
          Ready · 1 instance selected · Microsoft account linked
        </div>
      </div>
    </div>
  );
}
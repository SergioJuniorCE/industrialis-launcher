import { Server, Terminal } from "lucide-react";
import { cn } from "@/lib/utils";

const commands = [
  { cmd: "industrialis", desc: "Show help" },
  { cmd: "industrialis up", desc: "Start daemon + dashboard" },
  { cmd: "industrialis status", desc: "Process health and server list" },
  { cmd: "industrialis down", desc: "Stop daemon + dashboard" },
];

export function ServerHosting() {
  return (
    <section id="server" className="border-t border-border/60 py-20">
      <div className="mx-auto max-w-6xl px-6">
        <div className="mb-10 max-w-2xl">
          <p className="mb-2 inline-flex items-center gap-2 font-mono text-xs uppercase tracking-widest text-accent">
            <Server className="size-3.5" />
            Host a GTNH server
          </p>
          <h2 className="text-2xl font-semibold tracking-tight sm:text-3xl">Spin up GregTech servers from the CLI.</h2>
          <p className="mt-3 text-muted-foreground">
            Industrialis ships a Linux daemon, lightweight dashboard, and Docker integration for hosting GT New Horizons. Install once on your VPS, then create
            worlds from the browser UI.
          </p>
        </div>

        <div className="grid gap-6 lg:grid-cols-2">
          <div className="rounded-2xl border border-border bg-card p-6 sm:p-8">
            <p className="mb-3 font-mono text-xs uppercase tracking-widest text-muted-foreground">Install</p>
            <p className="mb-4 text-sm text-muted-foreground">
              Requires Linux, Docker Engine, and Node.js 22+. Prefer inspecting the script before piping to bash.
            </p>
            <pre
              className={cn(
                "overflow-x-auto rounded-lg border border-border bg-background p-4",
                "font-mono text-xs leading-relaxed text-foreground sm:text-sm",
              )}
            >
              <code>curl -fsSL https://industrialislauncher.yoggan.dev/install.sh | bash</code>
            </pre>
            <p className="mt-3 font-mono text-[11px] text-muted-foreground">
              Script: {/* eslint-disable-next-line react-doctor/nextjs-no-a-element -- install.sh is a static script download, not an internal route. */}
              <a href="/install.sh" className="text-primary underline-offset-2 hover:underline">
                /install.sh
              </a>
            </p>
          </div>

          <div className="rounded-2xl border border-border bg-card p-6 sm:p-8">
            <p className="mb-3 inline-flex items-center gap-2 font-mono text-xs uppercase tracking-widest text-muted-foreground">
              <Terminal className="size-3.5" />
              Commands
            </p>
            <ul className="space-y-3">
              {commands.map((item) => (
                <li
                  key={item.cmd}
                  className="flex flex-col gap-0.5 border-b border-border/60 pb-3 last:border-0 last:pb-0 sm:flex-row sm:items-baseline sm:justify-between sm:gap-4"
                >
                  <code className="font-mono text-sm text-primary">{item.cmd}</code>
                  <span className="text-sm text-muted-foreground">{item.desc}</span>
                </li>
              ))}
            </ul>
            <p className="mt-5 text-sm text-muted-foreground">
              After <code className="text-foreground">up</code>, open <code className="text-foreground">http://127.0.0.1:3001</code> (loopback). Create, start,
              and stop GTNH containers from the dashboard.
            </p>
          </div>
        </div>
      </div>
    </section>
  );
}

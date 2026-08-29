import Link from "next/link";
import { Server, Terminal } from "lucide-react";

const commands = [
  { cmd: "industrialis", desc: "Show help" },
  { cmd: "industrialis up", desc: "Start daemon and dashboard" },
  { cmd: "industrialis status", desc: "Process health and server list" },
  { cmd: "industrialis down", desc: "Stop daemon and dashboard" },
];

export function ServerHosting() {
  return (
    <section id="server" className="section-shell section-shell-tight">
      <div className="mx-auto max-w-7xl px-5 sm:px-8">
        <div className="server-panel">
          <div className="max-w-xl">
            <p className="eyebrow">Also included</p>
            <h2 className="mt-5 text-3xl font-semibold leading-tight tracking-[-0.04em] sm:text-4xl">Host a GTNH server from the same project.</h2>
            <p className="mt-4 leading-relaxed text-muted-foreground">
              The Linux daemon, lightweight dashboard, and Docker integration give you a practical way to create and manage worlds on a VPS.
            </p>
            <p className="mt-8 text-sm leading-relaxed text-muted-foreground">
              After <code className="code-inline">up</code>, open <code className="code-inline">http://127.0.0.1:3001</code> to create, start, and stop GTNH
              containers from the dashboard.
            </p>
          </div>

          <div className="server-terminal">
            <div className="flex items-center gap-2 border-b border-border px-4 py-3 font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
              <Terminal className="size-3.5 text-primary" />
              Server commands
            </div>
            <pre className="overflow-x-auto px-4 py-5 font-mono text-xs leading-relaxed text-foreground sm:px-6 sm:text-sm">
              <code>
                <span className="text-muted-foreground">$ </span>
                curl -fsSL https://industrialislauncher.yoggan.dev/install.sh | bash
                {"\n\n"}
                {commands.map(({ cmd, desc }) => (
                  <span key={cmd} className="block">
                    <span className="text-primary">$ {cmd}</span>
                    <span className="text-muted-foreground"> # {desc}</span>
                  </span>
                ))}
              </code>
            </pre>
            <Link
              href="/install.sh"
              className="inline-flex items-center gap-2 border-t border-border px-4 py-3 font-mono text-[11px] text-primary transition-colors hover:text-foreground sm:px-6"
            >
              <Server className="size-3.5" />
              Inspect the install script
            </Link>
          </div>
        </div>
      </div>
    </section>
  );
}

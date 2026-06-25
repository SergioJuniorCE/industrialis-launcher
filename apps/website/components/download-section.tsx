import { Download, ExternalLink } from "lucide-react";
import { cn } from "@/lib/utils";

export function DownloadSection() {
  return (
    <section id="download" className="border-t border-border/60 py-20">
      <div className="mx-auto max-w-6xl px-6">
        <div className="rounded-2xl border border-border bg-card p-8 sm:p-10">
          <div className="flex flex-col gap-8 lg:flex-row lg:items-center lg:justify-between">
            <div className="max-w-xl">
              <p className="mb-2 font-mono text-xs uppercase tracking-widest text-accent">
                Download
              </p>
              <h2 className="text-2xl font-semibold tracking-tight sm:text-3xl">
                Ready to install GTNH?
              </h2>
              <p className="mt-3 text-muted-foreground">
                Windows builds ship from this repo. macOS and Linux support is on
                the roadmap — the launcher is Tauri, so ports are straightforward.
              </p>
            </div>

            <div className="flex flex-col gap-3 sm:flex-row lg:flex-col xl:flex-row">
              <a
                href="#download"
                className={cn(
                  "inline-flex items-center justify-center gap-2 rounded-lg bg-primary px-5 py-3",
                  "text-sm font-semibold text-primary-foreground transition-opacity hover:opacity-90"
                )}
              >
                <Download className="size-4" />
                Download for Windows
              </a>
              <a
                href="https://gtnewhorizons.com/"
                target="_blank"
                rel="noreferrer"
                className={cn(
                  "inline-flex items-center justify-center gap-2 rounded-lg border border-border",
                  "px-5 py-3 text-sm font-medium text-muted-foreground transition-colors",
                  "hover:border-foreground/20 hover:text-foreground"
                )}
              >
                About GT New Horizons
                <ExternalLink className="size-3.5" />
              </a>
            </div>
          </div>

          <p className="mt-8 border-t border-border pt-6 font-mono text-xs text-muted-foreground">
            Requires Java 17+ to run GTNH instances · Microsoft account with
            Minecraft Java Edition for online play
          </p>
        </div>
      </div>
    </section>
  );
}
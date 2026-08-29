import { Download, ExternalLink } from "lucide-react";

const WINDOWS_DOWNLOAD_URL = "https://github.com/SergioJuniorCE/industrialis-launcher/releases/latest/download/IndustrialisLauncherSetup.exe";

export function DownloadSection() {
  return (
    <section id="download" className="download-section">
      <div className="mx-auto max-w-7xl px-5 sm:px-8">
        <div className="download-panel">
          <div className="max-w-2xl">
            <p className="eyebrow">Get started</p>
            <h2 className="mt-5 text-4xl font-semibold leading-[1.02] tracking-[-0.05em] sm:text-6xl">Make the next boot boring.</h2>
            <p className="mt-5 max-w-xl text-base leading-relaxed text-muted-foreground sm:text-lg">
              Download the Windows launcher, connect your account, and keep every GTNH instance in one place.
            </p>
          </div>

          <div className="mt-10 flex flex-wrap items-center gap-3">
            <a href={WINDOWS_DOWNLOAD_URL} target="_blank" rel="noreferrer" className="button-primary">
              <Download className="size-4" />
              Download for Windows
            </a>
            <a href="https://gtnewhorizons.com/" target="_blank" rel="noreferrer" className="button-secondary">
              About GT New Horizons
              <ExternalLink className="size-3.5" />
            </a>
          </div>

          <p className="mt-10 border-t border-border pt-5 font-mono text-[11px] leading-relaxed text-muted-foreground">
            Requires Java 17 or newer for current GTNH instances. Online play also requires a Microsoft account with Minecraft Java Edition.
          </p>
        </div>
      </div>
    </section>
  );
}

import { Download, ExternalLink } from "lucide-react";
import { DownloadSection } from "@/components/download-section";
import { Features } from "@/components/features";
import { LauncherPreview } from "@/components/launcher-preview";
import { ServerHosting } from "@/components/server-hosting";
import { SiteFooter } from "@/components/site-footer";
import { SiteHeader } from "@/components/site-header";
import { Workflow } from "@/components/workflow";

const GITHUB_URL = "https://github.com/SergioJuniorCE/industrialis-launcher";
const WINDOWS_DOWNLOAD_URL = "https://github.com/SergioJuniorCE/industrialis-launcher/releases/latest/download/IndustrialisLauncherSetup.exe";

export default function HomePage() {
  return (
    <>
      <SiteHeader />

      <main>
        <section className="hero-section">
          <div className="mx-auto max-w-7xl px-5 sm:px-8">
            <div className="hero-copy mx-auto max-w-5xl text-center">
              <p className="eyebrow">GT New Horizons launcher</p>
              <h1 className="mt-5 text-balance text-4xl font-semibold leading-[0.98] tracking-[-0.065em] sm:text-6xl lg:text-[4.75rem]">
                The control surface
                <br /> <span className="lg:whitespace-nowrap">for long modpack sessions.</span>
              </h1>
              <p className="mx-auto mt-6 max-w-2xl text-base leading-relaxed text-muted-foreground sm:text-lg">
                Install packs, tune Java and memory, then launch and inspect every session from one focused desktop workspace.
              </p>
              <div className="mt-8 flex flex-wrap items-center justify-center gap-3">
                <a href={WINDOWS_DOWNLOAD_URL} target="_blank" rel="noreferrer" className="button-primary">
                  <Download className="size-4" />
                  Download launcher
                </a>
                <a href={GITHUB_URL} target="_blank" rel="noreferrer" className="button-secondary">
                  View source
                  <ExternalLink className="size-3.5" />
                </a>
              </div>
            </div>

            <LauncherPreview />
          </div>
        </section>

        <Features />
        <Workflow />
        <ServerHosting />
        <DownloadSection />
      </main>

      <SiteFooter />
    </>
  );
}

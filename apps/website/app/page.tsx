import { ArrowRight } from "lucide-react";
import { DownloadSection } from "@/components/download-section";
import { Features } from "@/components/features";
import { LauncherPreview } from "@/components/launcher-preview";
import { SiteFooter } from "@/components/site-footer";
import { SiteHeader } from "@/components/site-header";
import { Workflow } from "@/components/workflow";
import { cn } from "@/lib/utils";

export default function HomePage() {
  return (
    <>
      <SiteHeader />

      <main>
        <section className="mx-auto max-w-6xl px-6 pb-16 pt-16 sm:pt-24">
          <div className="grid items-center gap-12 lg:grid-cols-2 lg:gap-16">
            <div>
              <p className="mb-4 inline-flex items-center gap-2 rounded-full border border-border bg-card px-3 py-1 font-mono text-xs text-muted-foreground">
                <span className="size-1.5 rounded-full bg-accent" />
                GT New Horizons
              </p>
              <h1 className="text-4xl font-semibold leading-[1.08] tracking-tight sm:text-5xl lg:text-[3.25rem]">
                Launch long modpack sessions without the friction.
              </h1>
              <p className="mt-5 max-w-lg text-base leading-relaxed text-muted-foreground sm:text-lg">
                Industrialis is a desktop launcher for GTNH — install instances,
                manage Java and memory, sign in with Microsoft, and watch your
                game output in one place.
              </p>
              <div className="mt-8 flex flex-wrap items-center gap-3">
                <a
                  href="#download"
                  className={cn(
                    "inline-flex items-center gap-2 rounded-lg bg-primary px-5 py-2.5",
                    "text-sm font-semibold text-primary-foreground transition-opacity hover:opacity-90"
                  )}
                >
                  Download launcher
                  <ArrowRight className="size-4" />
                </a>
                <a
                  href="#features"
                  className="inline-flex items-center rounded-lg border border-border px-5 py-2.5 text-sm font-medium text-muted-foreground transition-colors hover:text-foreground"
                >
                  See features
                </a>
              </div>
            </div>

            <LauncherPreview />
          </div>
        </section>

        <Features />
        <Workflow />
        <DownloadSection />
      </main>

      <SiteFooter />
    </>
  );
}
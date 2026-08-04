import { ExternalLink } from "lucide-react";

const GITHUB_URL = "https://github.com/SergioJuniorCE/industrialis-launcher";

export function SiteFooter() {
  return (
    <footer className="border-t border-border py-8">
      <div className="mx-auto flex max-w-6xl flex-col items-start justify-between gap-4 px-6 text-sm text-muted-foreground sm:flex-row sm:items-center">
        <p>Industrialis — unofficial GT New Horizons launcher</p>
        <div className="flex flex-wrap items-center gap-4">
          <a
            href={GITHUB_URL}
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-1.5 transition-colors hover:text-foreground"
          >
            <ExternalLink className="size-4" />
            View on GitHub
          </a>
          <p className="font-mono text-xs">
            Not affiliated with GTNH or Mojang
          </p>
        </div>
      </div>
    </footer>
  );
}

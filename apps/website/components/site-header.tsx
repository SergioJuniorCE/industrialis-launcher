import { Boxes, ExternalLink } from "lucide-react";

const GITHUB_URL = "https://github.com/SergioJuniorCE/industrialis-launcher";

const links = [
  { href: "#features", label: "Features" },
  { href: "#workflow", label: "Workflow" },
  { href: "#server", label: "Server" },
];

export function SiteHeader() {
  return (
    <header className="site-header">
      <div className="mx-auto flex h-16 max-w-7xl items-center justify-between gap-5 px-5 sm:px-8">
        <a href="#top" className="flex shrink-0 items-center gap-2.5 font-semibold tracking-tight">
          <span className="brand-mark">
            <Boxes className="size-4 text-primary" strokeWidth={2} />
          </span>
          Industrialis
        </a>

        <nav className="flex items-center gap-3 text-[11px] text-muted-foreground sm:gap-7 sm:text-sm">
          {links.map((link) => (
            <a key={link.href} href={link.href} className={`transition-colors hover:text-foreground ${link.label === "Workflow" ? "max-[420px]:hidden" : ""}`}>
              {link.label}
            </a>
          ))}
        </nav>

        <a
          href={GITHUB_URL}
          target="_blank"
          rel="noreferrer"
          aria-label="View Industrialis on GitHub"
          className="inline-flex shrink-0 items-center gap-1.5 text-xs font-medium text-foreground transition-colors hover:text-primary sm:text-sm"
        >
          <span className="hidden sm:inline">GitHub</span>
          <ExternalLink className="size-3.5" />
        </a>
      </div>
    </header>
  );
}

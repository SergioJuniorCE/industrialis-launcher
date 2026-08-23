import { Boxes } from "lucide-react";
import Link from "next/link";
import { cn } from "@/lib/utils";

const links = [
  { href: "/#features", label: "Features" },
  { href: "/docs/cloud-backups", label: "Cloud backups" },
  { href: "/#server", label: "Host a server" },
  { href: "/#download", label: "Download" },
];

export function SiteHeader() {
  return (
    <header className="sticky top-0 z-50 border-b border-border/80 bg-background/80 backdrop-blur-md">
      <div className="mx-auto flex h-14 max-w-6xl items-center justify-between px-6">
        <Link href="/" className="flex items-center gap-2.5 font-semibold tracking-tight">
          <span className="flex size-8 items-center justify-center rounded-md border border-border bg-card">
            <Boxes className="size-4 text-accent" strokeWidth={2.25} />
          </span>
          Industrialis
        </Link>
        <nav className="hidden items-center gap-8 text-sm text-muted-foreground sm:flex">
          {links.map((link) => (
            <Link key={link.href} href={link.href} className="transition-colors hover:text-foreground">
              {link.label}
            </Link>
          ))}
        </nav>
        <Link
          href="/#download"
          className={cn("rounded-md bg-primary px-3.5 py-1.5 text-sm font-medium text-primary-foreground", "transition-opacity hover:opacity-90")}
        >
          Get launcher
        </Link>
      </div>
    </header>
  );
}

export function SiteFooter() {
  return (
    <footer className="border-t border-border py-8">
      <div className="mx-auto flex max-w-6xl flex-col items-start justify-between gap-4 px-6 text-sm text-muted-foreground sm:flex-row sm:items-center">
        <p>Industrialis — unofficial GT New Horizons launcher</p>
        <p className="font-mono text-xs">
          Not affiliated with GTNH or Mojang
        </p>
      </div>
    </footer>
  );
}
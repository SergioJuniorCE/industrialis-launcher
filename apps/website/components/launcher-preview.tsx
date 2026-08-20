import Image from "next/image";

export function LauncherPreview() {
  return (
    <figure className="relative mx-auto w-full max-w-2xl">
      <div aria-hidden className="pointer-events-none absolute -inset-4 rounded-2xl bg-primary/10 blur-3xl" />
      <div className="relative overflow-hidden rounded-2xl border border-border bg-card shadow-2xl shadow-black/40">
        <Image
          src="/launcher-screenshot.png"
          alt="Industrialis Launcher showing a GTNH 2.9.0-beta-1 instance with version, size, Java, RAM, and authentication details."
          width={1102}
          height={782}
          priority
          sizes="(min-width: 1024px) 48vw, 100vw"
          className="h-auto w-full"
        />
      </div>
      <figcaption className="mt-3 flex flex-wrap items-center justify-between gap-2 px-1 text-xs text-muted-foreground">
        <span>Actual launcher screen - GTNH 2.9.0-beta-1</span>
        <span className="font-mono">Windows build</span>
      </figcaption>
    </figure>
  );
}

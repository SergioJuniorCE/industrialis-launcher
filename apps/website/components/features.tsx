import {
  Boxes,
  Coffee,
  Shield,
  Terminal,
  type LucideIcon,
} from "lucide-react";

const features: { icon: LucideIcon; title: string; body: string }[] = [
  {
    icon: Boxes,
    title: "Instance-first workflow",
    body: "Only installed GTNH versions appear in your library. Add new instances when you are ready — no clutter from packs you do not use.",
  },
  {
    icon: Coffee,
    title: "Java handled for you",
    body: "Detects Java on PATH, JAVA_HOME, and common install paths. Override per instance when a pack needs Java 8 or 17.",
  },
  {
    icon: Shield,
    title: "Microsoft authentication",
    body: "Sign in with your Microsoft account. OAuth in the browser with device-code fallback, plus automatic token refresh.",
  },
  {
    icon: Terminal,
    title: "Launch console",
    body: "Live stdout and stderr from every launch, persisted per instance so you can debug without digging through log folders.",
  },
];

export function Features() {
  return (
    <section id="features" className="border-t border-border/60 py-20">
      <div className="mx-auto max-w-6xl px-6">
        <div className="mb-12 max-w-2xl">
          <p className="mb-2 font-mono text-xs uppercase tracking-widest text-accent">
            Features
          </p>
          <h2 className="text-2xl font-semibold tracking-tight sm:text-3xl">
            Everything you need for GTNH, nothing you do not.
          </h2>
          <p className="mt-3 text-muted-foreground">
            Built around how long modpack sessions actually work — install once,
            tune RAM and JVM args, launch, and read the console when something
            breaks.
          </p>
        </div>

        <div className="grid gap-4 sm:grid-cols-2">
          {features.map(({ icon: Icon, title, body }) => (
            <article
              key={title}
              className="rounded-xl border border-border bg-card/60 p-5 transition-colors hover:border-border hover:bg-card"
            >
              <div className="mb-4 flex size-10 items-center justify-center rounded-lg border border-border bg-muted">
                <Icon className="size-5 text-accent" strokeWidth={1.75} />
              </div>
              <h3 className="mb-2 font-medium">{title}</h3>
              <p className="text-sm leading-relaxed text-muted-foreground">
                {body}
              </p>
            </article>
          ))}
        </div>
      </div>
    </section>
  );
}
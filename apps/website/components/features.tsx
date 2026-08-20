import { Boxes, Coffee, Terminal, Users, type LucideIcon } from "lucide-react";

const features: { icon: LucideIcon; title: string; body: string }[] = [
  {
    icon: Boxes,
    title: "A library that stays focused",
    body: "Install the GTNH versions you want and keep each pack in its own instance. The fixed-card grid adapts to the window without stretching everything out.",
  },
  {
    icon: Coffee,
    title: "Runtime controls per pack",
    body: "Choose the Java runtime, RAM, JVM arguments, and window behavior close to the instance that needs them. Settings stay with the pack.",
  },
  {
    icon: Users,
    title: "Accounts when you need them",
    body: "Manage Microsoft and offline profiles in the launcher, set a default account, or override the account for a specific instance.",
  },
  {
    icon: Terminal,
    title: "Launch state you can inspect",
    body: "Follow installs, updates, and launches from the Processes view, then open an instance's logs when a long boot needs troubleshooting.",
  },
];

export function Features() {
  return (
    <section id="features" className="border-t border-border/60 py-20">
      <div className="mx-auto max-w-6xl px-6">
        <div className="mb-12 max-w-2xl">
          <p className="mb-2 font-mono text-xs uppercase tracking-widest text-accent">Features</p>
          <h2 className="text-2xl font-semibold tracking-tight sm:text-3xl">The controls that matter once the pack gets complicated.</h2>
          <p className="mt-3 text-muted-foreground">
            Built around the real loop: install a version, tune its runtime, launch, and see what happened when something breaks.
          </p>
        </div>

        <div className="grid gap-4 sm:grid-cols-2">
          {features.map(({ icon: Icon, title, body }) => (
            <article key={title} className="rounded-xl border border-border bg-card/60 p-5 transition-colors hover:border-border hover:bg-card">
              <div className="mb-4 flex size-10 items-center justify-center rounded-lg border border-border bg-muted">
                <Icon className="size-5 text-accent" strokeWidth={1.75} />
              </div>
              <h3 className="mb-2 font-medium">{title}</h3>
              <p className="text-sm leading-relaxed text-muted-foreground">{body}</p>
            </article>
          ))}
        </div>
      </div>
    </section>
  );
}

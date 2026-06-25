const steps = [
  {
    step: "01",
    title: "Pick a version",
    body: "Browse stable and beta GTNH releases from the official manifest and install with one click.",
  },
  {
    step: "02",
    title: "Configure",
    body: "Set RAM, JVM arguments, Java path, and auth mode per instance. Theme the launcher while you are at it.",
  },
  {
    step: "03",
    title: "Launch",
    body: "Hit Play. Watch the console in-app, fix issues fast, and get back to GregTech.",
  },
];

export function Workflow() {
  return (
    <section id="workflow" className="py-20">
      <div className="mx-auto max-w-6xl px-6">
        <div className="mb-12 max-w-2xl">
          <p className="mb-2 font-mono text-xs uppercase tracking-widest text-accent">
            Workflow
          </p>
          <h2 className="text-2xl font-semibold tracking-tight sm:text-3xl">
            From download to first boot in three steps.
          </h2>
        </div>

        <ol className="grid gap-6 md:grid-cols-3">
          {steps.map(({ step, title, body }) => (
            <li
              key={step}
              className="relative rounded-xl border border-border bg-card/40 p-6"
            >
              <span className="font-mono text-3xl font-bold text-muted-foreground/30">
                {step}
              </span>
              <h3 className="mt-2 mb-2 text-lg font-medium">{title}</h3>
              <p className="text-sm leading-relaxed text-muted-foreground">
                {body}
              </p>
            </li>
          ))}
        </ol>
      </div>
    </section>
  );
}
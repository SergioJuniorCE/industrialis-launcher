const steps = [
  {
    step: "01",
    title: "Add an instance",
    body: "Choose a GTNH release, give it a name, and keep it in the instance library with its own group and icon.",
  },
  {
    step: "02",
    title: "Tune the runtime",
    body: "Set the Java runtime, RAM, JVM arguments, account, and launch-window behavior for the pack you are configuring.",
  },
  {
    step: "03",
    title: "Launch and inspect",
    body: "Start the instance, follow background work in Processes, and open its logs when you need to understand a failed boot.",
  },
];

export function Workflow() {
  return (
    <section id="workflow" className="py-20">
      <div className="mx-auto max-w-6xl px-6">
        <div className="mb-12 max-w-2xl">
          <p className="mb-2 font-mono text-xs uppercase tracking-widest text-accent">Workflow</p>
          <h2 className="text-2xl font-semibold tracking-tight sm:text-3xl">From a fresh pack to a repeatable launch.</h2>
        </div>

        <ol className="grid gap-6 md:grid-cols-3">
          {steps.map(({ step, title, body }) => (
            <li key={step} className="relative rounded-xl border border-border bg-card/40 p-6">
              <span className="font-mono text-3xl font-bold text-muted-foreground">{step}</span>
              <h3 className="mt-2 mb-2 text-lg font-medium">{title}</h3>
              <p className="text-sm leading-relaxed text-muted-foreground">{body}</p>
            </li>
          ))}
        </ol>
      </div>
    </section>
  );
}

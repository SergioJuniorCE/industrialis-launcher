import { Boxes, Play, SlidersHorizontal } from "lucide-react";

const steps = [
  {
    icon: Boxes,
    title: "Pick a version",
    body: "Browse stable and beta GTNH releases from the official manifest.",
  },
  {
    icon: SlidersHorizontal,
    title: "Configure the instance",
    body: "Set RAM, JVM arguments, Java path, and auth mode where they belong.",
  },
  {
    icon: Play,
    title: "Launch and inspect",
    body: "Start the game, watch the console, and get back to GregTech quickly.",
  },
];

export function Workflow() {
  return (
    <section id="workflow" className="section-shell section-shell-tight">
      <div className="mx-auto max-w-7xl px-5 sm:px-8">
        <div className="section-heading max-w-2xl">
          <h2 className="text-3xl font-semibold leading-tight tracking-[-0.04em] sm:text-4xl">Download to first boot, without the scavenger hunt.</h2>
        </div>

        <ol className="workflow-list mt-12 grid md:grid-cols-3">
          {steps.map(({ icon: Icon, title, body }) => (
            <li key={title} className="workflow-item">
              <div className="workflow-icon" aria-hidden>
                <Icon className="size-4" strokeWidth={1.75} />
              </div>
              <h3 className="mt-5 text-lg font-medium tracking-[-0.02em]">{title}</h3>
              <p className="mt-2 text-sm leading-relaxed text-muted-foreground">{body}</p>
            </li>
          ))}
        </ol>
      </div>
    </section>
  );
}

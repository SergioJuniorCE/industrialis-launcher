import Image from "next/image";
import { Boxes, Coffee, Terminal, Users, type LucideIcon } from "lucide-react";

const features: { icon: LucideIcon; title: string; body: string }[] = [
  {
    icon: Boxes,
    title: "A library that stays focused",
    body: "Install the GTNH versions you want and keep each pack in its own instance. Only installed versions stay in the library.",
  },
  {
    icon: Coffee,
    title: "Runtime controls per pack",
    body: "Detect Java from PATH, JAVA_HOME, and common install locations, then choose the runtime, RAM, JVM arguments, and window behavior per instance.",
  },
  {
    icon: Users,
    title: "Accounts when you need them",
    body: "Manage Microsoft and offline profiles, set a default account or override it per instance, and stay signed in with automatic token refresh.",
  },
  {
    icon: Terminal,
    title: "Launch state you can inspect",
    body: "Follow installs, updates, and launches from the Processes view, and read live stdout and stderr — persisted per instance for the next time something breaks.",
  },
];

export function Features() {
  return (
    <section id="features" className="section-shell">
      <div className="mx-auto max-w-7xl px-5 sm:px-8">
        <div className="section-heading max-w-2xl">
          <h2 className="text-3xl font-semibold leading-tight tracking-[-0.04em] sm:text-4xl">The useful parts stay close.</h2>
          <p className="mt-4 text-base leading-relaxed text-muted-foreground">
            Industrialis keeps the setup work visible, local, and easy to pick up again after a long break.
          </p>
        </div>

        <div className="mt-14 grid gap-10 lg:grid-cols-[0.9fr_1.1fr] lg:items-start lg:gap-16">
          <div className="feature-image-frame">
            <Image
              src="/industrialis-control-room.png"
              alt="Stylized industrial control room with copper machinery and technical drawings"
              width={1536}
              height={1024}
              className="h-full w-full object-cover"
            />
          </div>

          <div className="feature-list">
            {features.map(({ icon: Icon, title, body }) => (
              <article key={title} className="feature-row">
                <div className="feature-icon" aria-hidden>
                  <Icon className="size-4" strokeWidth={1.75} />
                </div>
                <div>
                  <h3 className="text-lg font-medium tracking-[-0.02em]">{title}</h3>
                  <p className="mt-2 max-w-xl text-sm leading-relaxed text-muted-foreground">{body}</p>
                </div>
              </article>
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}

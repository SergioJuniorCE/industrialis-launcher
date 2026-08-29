import Image from "next/image";
import { Boxes, Coffee, Shield, Terminal, type LucideIcon } from "lucide-react";

const features: { icon: LucideIcon; title: string; body: string }[] = [
  {
    icon: Boxes,
    title: "Keep the library clean",
    body: "Only installed GTNH versions stay in the library. Add another instance when you actually need it.",
  },
  {
    icon: Coffee,
    title: "Use the right Java",
    body: "Detect Java from PATH, JAVA_HOME, and common install locations. Override it per instance when a pack needs a specific version.",
  },
  {
    icon: Shield,
    title: "Sign in without friction",
    body: "Connect a Microsoft account in the browser with a device-code fallback and automatic token refresh.",
  },
  {
    icon: Terminal,
    title: "See what the game is doing",
    body: "Read live stdout and stderr in the launcher. Output is persisted per instance for the next time something breaks.",
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

import Image from "next/image";
import { Boxes, Cog, Terminal, Zap } from "lucide-react";

function FloatingMark({
  className,
  children,
}: Readonly<{
  className: string;
  children: React.ReactNode;
}>) {
  return (
    <div aria-hidden className={`floating-mark ${className}`}>
      {children}
    </div>
  );
}

export function LauncherPreview() {
  return (
    <figure className="hero-preview-wrap">
      <FloatingMark className="floating-mark-one">
        <Boxes className="size-7" strokeWidth={1.5} />
      </FloatingMark>
      <FloatingMark className="floating-mark-two">
        <Cog className="size-7" strokeWidth={1.5} />
      </FloatingMark>
      <FloatingMark className="floating-mark-three">
        <Terminal className="size-7" strokeWidth={1.5} />
      </FloatingMark>
      <FloatingMark className="floating-mark-four">
        <Zap className="size-7" strokeWidth={1.5} />
      </FloatingMark>

      <div className="hero-preview-frame">
        <Image
          src="/launcher-screenshot.png"
          alt="Industrialis launcher showing GTNH 2.9.0 running with the Logs view open"
          width={1602}
          height={901}
          priority
          className="h-auto w-full"
        />
      </div>
      <figcaption className="mt-3 text-center font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground/70">Live launcher view</figcaption>
    </figure>
  );
}

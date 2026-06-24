export default function HomePage() {
  return (
    <main>
      <header
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          padding: "1.25rem 2rem",
          borderBottom: "1px solid var(--border)",
        }}
      >
        <strong style={{ fontSize: "1.1rem", letterSpacing: "0.02em" }}>
          Industrialis
        </strong>
        <nav style={{ display: "flex", gap: "1.5rem", color: "var(--muted)" }}>
          <a href="#features">Features</a>
          <a href="#download">Download</a>
        </nav>
      </header>

      <section
        style={{
          maxWidth: 960,
          margin: "0 auto",
          padding: "5rem 2rem 3rem",
        }}
      >
        <p
          style={{
            color: "var(--accent)",
            fontWeight: 600,
            fontSize: "0.875rem",
            textTransform: "uppercase",
            letterSpacing: "0.08em",
            marginBottom: "1rem",
          }}
        >
          GT New Horizons
        </p>
        <h1
          style={{
            fontSize: "clamp(2.25rem, 5vw, 3.5rem)",
            lineHeight: 1.1,
            margin: "0 0 1.25rem",
            maxWidth: 720,
          }}
        >
          A launcher built for long modpack sessions.
        </h1>
        <p
          style={{
            color: "var(--muted)",
            fontSize: "1.125rem",
            lineHeight: 1.6,
            maxWidth: 620,
            marginBottom: "2rem",
          }}
        >
          Install GTNH instances, manage Java and memory, authenticate with
          Microsoft, and launch from one desktop app.
        </p>
        <div id="download" style={{ display: "flex", gap: "1rem", flexWrap: "wrap" }}>
          <a
            href="#download"
            style={{
              background: "var(--accent)",
              color: "white",
              padding: "0.75rem 1.25rem",
              borderRadius: 8,
              fontWeight: 600,
            }}
          >
            Download launcher
          </a>
          <a
            href="https://gtnewhorizons.com/"
            target="_blank"
            rel="noreferrer"
            style={{
              border: "1px solid var(--border)",
              padding: "0.75rem 1.25rem",
              borderRadius: 8,
              color: "var(--muted)",
            }}
          >
            About GTNH
          </a>
        </div>
      </section>

      <section
        id="features"
        style={{
          maxWidth: 960,
          margin: "0 auto",
          padding: "2rem 2rem 5rem",
          display: "grid",
          gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))",
          gap: "1rem",
        }}
      >
        {[
          {
            title: "Instance management",
            body: "Browse stable and beta GTNH versions, install, rename, and remove instances.",
          },
          {
            title: "Microsoft sign-in",
            body: "OAuth and device-code flows with automatic token refresh.",
          },
          {
            title: "Launch console",
            body: "Live game output with per-instance logs persisted on disk.",
          },
        ].map((feature) => (
          <article
            key={feature.title}
            style={{
              background: "var(--bg-elevated)",
              border: "1px solid var(--border)",
              borderRadius: 12,
              padding: "1.25rem",
            }}
          >
            <h2 style={{ fontSize: "1rem", margin: "0 0 0.5rem" }}>
              {feature.title}
            </h2>
            <p style={{ margin: 0, color: "var(--muted)", lineHeight: 1.5 }}>
              {feature.body}
            </p>
          </article>
        ))}
      </section>
    </main>
  );
}
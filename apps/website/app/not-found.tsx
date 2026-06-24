export default function NotFound() {
  return (
    <main style={{ padding: "4rem 2rem", maxWidth: 640, margin: "0 auto" }}>
      <h1 style={{ margin: "0 0 0.5rem" }}>Page not found</h1>
      <p style={{ color: "var(--muted)", margin: 0 }}>
        <a href="/" style={{ textDecoration: "underline" }}>
          Back to home
        </a>
      </p>
    </main>
  );
}
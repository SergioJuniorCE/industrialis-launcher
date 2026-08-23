import type { Metadata } from "next";
import Link from "next/link";
import { ArrowLeft, CheckCircle2, Clock3, ExternalLink, ShieldCheck } from "lucide-react";
import { SiteFooter } from "@/components/site-footer";
import { SiteHeader } from "@/components/site-header";

const REPOSITORY_GUIDE_URL = "https://github.com/SergioJuniorCE/industrialis-launcher/blob/master/docs/cloud-backups.md";

const setupSteps = [
  {
    title: "Create or select a Google Cloud project",
    body: "Use a project you control, then enable the Google Drive API for that project.",
    link: "https://console.cloud.google.com/apis/library/drive.googleapis.com",
    linkLabel: "Enable the Google Drive API",
  },
  {
    title: "Configure Google Auth Platform",
    body: "Add an app name and support email. For an External app in Testing, add the Google account you will connect as a test user.",
    link: "https://console.cloud.google.com/auth/overview",
    linkLabel: "Open Google Auth Platform",
  },
  {
    title: "Declare the Drive permission",
    body: "Under Data Access, add the non-sensitive drive.file scope. It limits Industrialis to files the app creates or that you explicitly share with it.",
    link: "https://developers.google.com/workspace/drive/api/guides/api-specific-auth",
    linkLabel: "Review Google Drive scopes",
  },
  {
    title: "Create a Desktop app client",
    body: "Open Clients, choose Create client, select Desktop app, and copy the client ID. You do not need to paste a client secret or configure a redirect URL.",
    link: "https://console.cloud.google.com/auth/clients",
    linkLabel: "Create an OAuth client",
  },
  {
    title: "Connect the launcher",
    body: "Open Launcher Settings → Backups, paste the client ID, choose Save ID, then Connect. Finish consent in the browser that opens.",
  },
  {
    title: "Enable each instance",
    body: "Open Instance Settings → General and turn on Cloud backups. Stable files in that instance's backups folder are uploaded automatically.",
  },
] as const;

const troubleshooting = [
  ["Access blocked or access_denied", "If the OAuth app is in Testing, add the account under Google Auth Platform → Audience → Test users."],
  [
    "Connection stops after seven days",
    "Google expires authorizations for External apps left in Testing. Move the app to In production for a durable connection.",
  ],
  ["invalid_client", "Confirm that you copied the client ID from an OAuth client whose application type is Desktop app."],
  [
    "Authorization times out",
    "Retry Connect and allow local 127.0.0.1 traffic. The launcher briefly opens a random loopback port to receive Google's response.",
  ],
] as const;

export const metadata: Metadata = {
  title: "Connect cloud backups - Industrialis Launcher",
  description: "Set up Google Drive cloud backups for Industrialis Launcher and see the providers planned next.",
};

export default function CloudBackupsGuidePage() {
  return (
    <>
      <SiteHeader />
      <main className="mx-auto max-w-5xl px-6 py-14 sm:py-20">
        <Link href="/" className="inline-flex items-center gap-1.5 text-sm text-muted-foreground transition-colors hover:text-foreground">
          <ArrowLeft className="size-4" /> Back to Industrialis
        </Link>

        <div className="mt-8 max-w-3xl">
          <p className="font-mono text-xs uppercase tracking-widest text-accent">Cloud backup guide</p>
          <h1 className="mt-3 text-3xl font-semibold tracking-tight sm:text-4xl">Connect Google Drive to your launcher.</h1>
          <p className="mt-4 text-base leading-relaxed text-muted-foreground">
            Google Drive is the first available provider. Setup takes a Google Cloud project and a Desktop app OAuth client ID; no client secret is required.
          </p>
        </div>

        <section className="mt-10 grid gap-4 sm:grid-cols-3" aria-label="Backup behavior">
          <article className="rounded-xl border border-border bg-card/60 p-5">
            <CheckCircle2 className="size-5 text-accent" />
            <h2 className="mt-3 font-medium">Automatic</h2>
            <p className="mt-1 text-sm leading-relaxed text-muted-foreground">
              New stable files from an enabled instance&apos;s backups folder are queued in the background.
            </p>
          </article>
          <article className="rounded-xl border border-border bg-card/60 p-5">
            <ShieldCheck className="size-5 text-accent" />
            <h2 className="mt-3 font-medium">Limited Drive access</h2>
            <p className="mt-1 text-sm leading-relaxed text-muted-foreground">
              The launcher requests only <code className="text-foreground">drive.file</code>, not access to every file in your Drive.
            </p>
          </article>
          <article className="rounded-xl border border-border bg-card/60 p-5">
            <Clock3 className="size-5 text-accent" />
            <h2 className="mt-3 font-medium">Safe restore</h2>
            <p className="mt-1 text-sm leading-relaxed text-muted-foreground">
              Restores download an archive into the local backups folder and never replace the live world.
            </p>
          </article>
        </section>

        <section className="mt-14">
          <h2 className="text-2xl font-semibold tracking-tight">Google Drive setup</h2>
          <ol className="mt-6 space-y-4">
            {setupSteps.map((step, index) => (
              <li key={step.title} className="flex gap-4 rounded-xl border border-border bg-card/50 p-5">
                <span className="flex size-8 shrink-0 items-center justify-center rounded-full bg-primary font-mono text-sm font-semibold text-primary-foreground">
                  {index + 1}
                </span>
                <div>
                  <h3 className="font-medium">{step.title}</h3>
                  <p className="mt-1 text-sm leading-relaxed text-muted-foreground">{step.body}</p>
                  {"link" in step ? (
                    <a href={step.link} target="_blank" rel="noreferrer" className="mt-2 inline-flex items-center gap-1.5 text-sm text-accent hover:underline">
                      {step.linkLabel} <ExternalLink className="size-3.5" />
                    </a>
                  ) : null}
                </div>
              </li>
            ))}
          </ol>
        </section>

        <section className="mt-14 grid gap-8 lg:grid-cols-[1.1fr_0.9fr]">
          <div>
            <h2 className="text-2xl font-semibold tracking-tight">Troubleshooting</h2>
            <div className="mt-5 divide-y divide-border rounded-xl border border-border bg-card/50">
              {troubleshooting.map(([problem, resolution]) => (
                <article key={problem} className="p-5">
                  <h3 className="text-sm font-medium">{problem}</h3>
                  <p className="mt-1 text-sm leading-relaxed text-muted-foreground">{resolution}</p>
                </article>
              ))}
            </div>
          </div>

          <aside className="rounded-xl border border-border bg-muted/40 p-6">
            <h2 className="font-medium">Storage and security</h2>
            <ul className="mt-3 space-y-2 text-sm leading-relaxed text-muted-foreground">
              <li>
                Backups appear under <span className="text-foreground">Industrialis Backups</span> in Google Drive.
              </li>
              <li>The OAuth refresh token is protected with the operating system&apos;s secure storage.</li>
              <li>Minecraft backup archives are uploaded as-is and are not additionally encrypted.</li>
              <li>Disconnecting a provider does not delete its remote backups.</li>
            </ul>
            <a
              href={REPOSITORY_GUIDE_URL}
              target="_blank"
              rel="noreferrer"
              className="mt-5 inline-flex items-center gap-1.5 text-sm text-accent hover:underline"
            >
              Full repository guide <ExternalLink className="size-3.5" />
            </a>
          </aside>
        </section>

        <section className="mt-14 border-t border-border pt-10">
          <p className="font-mono text-xs uppercase tracking-widest text-accent">Planned providers</p>
          <h2 className="mt-2 text-2xl font-semibold tracking-tight">More connections are coming.</h2>
          <p className="mt-3 max-w-3xl text-sm leading-relaxed text-muted-foreground">
            S3-compatible storage, OneDrive, FTP/FTPS, and SFTP will use the same snapshot format and backup workflow. Their connection guides will be published
            when each provider is available.
          </p>
        </section>
      </main>
      <SiteFooter />
    </>
  );
}

import { useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";
import type { CreateServerInput, GtnhServer, ServerLog } from "@industrialis/server-contracts";
import useSWR from "swr";
import {
  Activity,
  Box,
  ChevronRight,
  CircleAlert,
  Command,
  FileTerminal,
  LoaderCircle,
  Play,
  Plus,
  RotateCw,
  Server,
  Square,
  Trash2,
  X,
} from "lucide-react";

const API_URL = "/api/daemon";

async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${API_URL}${path}`, {
    ...init,
    headers: { "content-type": "application/json", ...init?.headers },
  });
  if (!response.ok) {
    const body = (await response.json().catch(() => null)) as { error?: string } | null;
    throw new Error(body?.error ?? `${response.status} ${response.statusText}`);
  }
  if (response.status === 204) return undefined as T;
  return response.json() as Promise<T>;
}

function Status({ status }: { status: GtnhServer["status"] }) {
  const active = status === "running";
  return (
    <span className={`inline-flex items-center gap-2 text-xs ${active ? "text-online" : "text-dim"}`}>
      <span className={`size-1.5 rounded-full ${active ? "bg-online" : "bg-dim/50"}`} />
      <span className="capitalize">{status}</span>
    </span>
  );
}

function IconButton({
  label,
  disabled,
  danger,
  onClick,
  children,
}: {
  label: string;
  disabled?: boolean;
  danger?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      disabled={disabled}
      onClick={onClick}
      className={`grid size-8 place-items-center border border-line bg-panel transition hover:border-dim hover:bg-panel-raised disabled:cursor-not-allowed disabled:opacity-35 ${danger ? "text-danger" : "text-copy"}`}
    >
      {children}
    </button>
  );
}

function Modal({
  onClose,
  label,
  labelledBy,
  children,
}: {
  onClose: () => void;
  label?: string;
  labelledBy?: string;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const dialog = ref.current;
    dialog?.showModal();
    return () => dialog?.close();
  }, []);

  return (
    <dialog
      ref={ref}
      aria-label={label}
      aria-labelledby={labelledBy}
      onCancel={onClose}
      className="fixed inset-0 m-auto max-h-[100dvh] w-full max-w-none border-0 bg-transparent p-3 text-copy backdrop:bg-black/70"
    >
      {children}
    </dialog>
  );
}

export default function FleetConsole() {
  const [pending, setPending] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [logs, setLogs] = useState<{ server: GtnhServer; value: string } | null>(null);
  const {
    data: servers = [],
    error: loadError,
    isLoading,
    isValidating,
    mutate,
  } = useSWR<GtnhServer[]>("/api/servers", api, { refreshInterval: 5000 });
  const error = actionError ?? (loadError instanceof Error ? loadError.message : null);

  async function run(id: string, action: "start" | "stop" | "restart" | "remove") {
    setPending(`${id}:${action}`);
    setActionError(null);
    try {
      await api(`/api/servers/${encodeURIComponent(id)}${action === "remove" ? "" : `/${action}`}`, {
        method: action === "remove" ? "DELETE" : "POST",
      });
      await mutate();
    } catch (requestError) {
      setActionError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setPending(null);
    }
  }

  async function showLogs(server: GtnhServer) {
    setPending(`${server.id}:logs`);
    try {
      const result = await api<ServerLog>(`/api/servers/${encodeURIComponent(server.id)}/logs?tail=300`);
      setLogs({ server, value: result.lines || "No log output yet." });
    } catch (requestError) {
      setActionError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setPending(null);
    }
  }

  const online = servers.filter((server) => server.status === "running").length;

  return (
    <main className="min-h-[100dvh] lg:grid lg:grid-cols-[220px_1fr]">
      <aside className="border-b border-line bg-panel/95 px-5 py-4 lg:min-h-[100dvh] lg:border-b-0 lg:border-r lg:py-6">
        <div className="flex items-center gap-3">
          <div className="grid size-8 place-items-center bg-signal text-ink">
            <Box className="size-4" />
          </div>
          <div>
            <p className="text-sm font-semibold tracking-tight">Industrialis</p>
            <p className="font-mono text-[9px] uppercase tracking-[0.2em] text-dim">Server console</p>
          </div>
        </div>
        <nav className="mt-5 flex gap-2 lg:mt-10 lg:block">
          <div className="flex items-center gap-3 border-l-2 border-signal bg-signal-dark px-3 py-2 text-xs font-medium">
            <Server className="size-4 text-signal" /> Fleet
          </div>
        </nav>
        <div className="mt-6 hidden border-t border-line pt-5 lg:block">
          <p className="font-mono text-[9px] uppercase tracking-[0.18em] text-dim">Daemon</p>
          <p className="mt-2 truncate font-mono text-[10px] text-copy">Authenticated local proxy</p>
        </div>
      </aside>

      <section className="min-w-0 px-4 py-6 sm:px-7 lg:px-10 lg:py-9">
        <header className="flex flex-col gap-5 border-b border-line pb-7 sm:flex-row sm:items-end sm:justify-between">
          <div>
            <p className="mb-2 font-mono text-[10px] uppercase tracking-[0.2em] text-signal">
              Operations / Fleet
            </p>
            <h1 className="text-3xl font-semibold tracking-[-0.04em] sm:text-4xl">GTNH servers</h1>
            <p className="mt-2 max-w-xl text-sm text-dim">
              Docker-isolated worlds, controlled from one host.
            </p>
          </div>
          <button
            type="button"
            onClick={() => setCreateOpen(true)}
            className="inline-flex h-10 items-center justify-center gap-2 bg-signal px-4 text-xs font-semibold text-ink transition hover:bg-[#dfbb4d] active:translate-y-px"
          >
            <Plus className="size-4" /> New server
          </button>
        </header>

        <div className="grid grid-cols-2 border-b border-line sm:grid-cols-3">
          <div className="border-r border-line py-5 pr-5">
            <p className="font-mono text-[9px] uppercase tracking-[0.18em] text-dim">Total</p>
            <p className="mt-1 text-2xl font-medium">{servers.length}</p>
          </div>
          <div className="border-r border-line px-5 py-5">
            <p className="font-mono text-[9px] uppercase tracking-[0.18em] text-dim">Online</p>
            <p className="mt-1 text-2xl font-medium text-online">{online}</p>
          </div>
          <div className="hidden px-5 py-5 sm:block">
            <p className="font-mono text-[9px] uppercase tracking-[0.18em] text-dim">Allocated memory</p>
            <p className="mt-1 text-2xl font-medium">
              {(servers.reduce((sum, server) => sum + server.memoryMb, 0) / 1024).toFixed(1)} GB
            </p>
          </div>
        </div>

        {error && (
          <div className="mt-5 flex items-start gap-3 border border-danger/40 bg-danger/5 px-4 py-3 text-xs text-danger">
            <CircleAlert className="mt-0.5 size-4 shrink-0" />
            <span className="flex-1">{error}</span>
            <button type="button" onClick={() => setActionError(null)} aria-label="Dismiss error">
              <X className="size-4" />
            </button>
          </div>
        )}

        <div className="mt-7">
          <div className="mb-3 grid grid-cols-[1fr_auto] items-center">
            <h2 className="text-sm font-semibold">Managed instances</h2>
            <button
              type="button"
              onClick={() => void mutate()}
              className="text-dim transition hover:text-copy"
              title="Refresh"
            >
              <RotateCw className={`size-4 ${isValidating ? "animate-spin" : ""}`} />
            </button>
          </div>

          {isLoading && servers.length === 0 ? (
            <div className="grid h-40 place-items-center border border-line">
              <LoaderCircle className="size-5 animate-spin text-signal" />
            </div>
          ) : servers.length === 0 ? (
            <button
              type="button"
              onClick={() => setCreateOpen(true)}
              className="group flex w-full items-center justify-between border border-dashed border-line px-5 py-8 text-left transition hover:border-dim"
            >
              <span>
                <span className="block text-sm font-medium">No servers configured</span>
                <span className="mt-1 block text-xs text-dim">
                  Create the first isolated GTNH world on this host.
                </span>
              </span>
              <ChevronRight className="size-5 text-dim transition group-hover:translate-x-1 group-hover:text-signal" />
            </button>
          ) : (
            <div className="border-t border-line">
              {servers.map((server) => {
                const busy = pending?.startsWith(`${server.id}:`) ?? false;
                return (
                  <article
                    key={server.id}
                    className="grid gap-4 border-b border-line py-4 sm:grid-cols-[minmax(180px,1.5fr)_1fr_1fr_auto] sm:items-center"
                  >
                    <div className="flex min-w-0 items-center gap-3">
                      <div className="grid size-9 shrink-0 place-items-center border border-line bg-panel font-mono text-xs text-signal">
                        GT
                      </div>
                      <div className="min-w-0">
                        <h3 className="truncate text-sm font-medium">{server.name}</h3>
                        <p className="truncate font-mono text-[10px] text-dim">{server.id}</p>
                      </div>
                    </div>
                    <div>
                      <Status status={server.status} />
                      <p className="mt-1 font-mono text-[10px] text-dim">localhost:{server.port}</p>
                    </div>
                    <div className="font-mono text-[10px] text-dim">
                      <p>{server.version}</p>
                      <p className="mt-1">{(server.memoryMb / 1024).toFixed(1)} GB limit</p>
                    </div>
                    <div className="flex items-center gap-1">
                      {server.status === "running" ? (
                        <IconButton
                          label="Stop"
                          disabled={busy}
                          onClick={() => void run(server.id, "stop")}
                        >
                          <Square className="size-3.5" />
                        </IconButton>
                      ) : (
                        <IconButton
                          label="Start"
                          disabled={busy || server.status === "missing"}
                          onClick={() => void run(server.id, "start")}
                        >
                          <Play className="size-3.5" />
                        </IconButton>
                      )}
                      <IconButton
                        label="Restart"
                        disabled={busy || server.status !== "running"}
                        onClick={() => void run(server.id, "restart")}
                      >
                        <RotateCw className="size-3.5" />
                      </IconButton>
                      <IconButton
                        label="Logs"
                        disabled={busy || !server.containerId}
                        onClick={() => void showLogs(server)}
                      >
                        <FileTerminal className="size-3.5" />
                      </IconButton>
                      <IconButton
                        label="Remove"
                        danger
                        disabled={busy}
                        onClick={() =>
                          window.confirm(
                            `Remove ${server.name}? World data will remain on disk.`,
                          ) && void run(server.id, "remove")
                        }
                      >
                        <Trash2 className="size-3.5" />
                      </IconButton>
                    </div>
                  </article>
                );
              })}
            </div>
          )}
        </div>
      </section>

      {createOpen && (
        <CreateServer
          onClose={() => setCreateOpen(false)}
          onCreated={() => {
            setCreateOpen(false);
            void mutate();
          }}
          onError={setActionError}
        />
      )}
      {logs && (
        <Modal onClose={() => setLogs(null)} label={`${logs.server.name} logs`}>
          <section className="mx-auto flex max-h-[82dvh] w-full max-w-4xl flex-col border border-line bg-ink shadow-2xl">
            <header className="flex items-center gap-3 border-b border-line px-4 py-3">
              <Command className="size-4 text-signal" />
              <div className="min-w-0 flex-1">
                <p className="truncate text-xs font-medium">{logs.server.name}</p>
                <p className="font-mono text-[9px] text-dim">Last 300 lines</p>
              </div>
              <button type="button" onClick={() => setLogs(null)} aria-label="Close logs">
                <X className="size-4" />
              </button>
            </header>
            <pre className="overflow-auto whitespace-pre-wrap p-4 font-mono text-[10px] leading-5 text-[#c7cbbf]">
              {logs.value}
            </pre>
          </section>
        </Modal>
      )}
    </main>
  );
}

function CreateServer({
  onClose,
  onCreated,
  onError,
}: {
  onClose: () => void;
  onCreated: () => void;
  onError: (error: string) => void;
}) {
  const [submitting, setSubmitting] = useState(false);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const input: CreateServerInput = {
      name: String(data.get("name")),
      version: String(data.get("version")),
      port: Number(data.get("port")),
      memoryMb: Number(data.get("memoryMb")),
    };
    setSubmitting(true);
    try {
      await api<GtnhServer>("/api/servers", { method: "POST", body: JSON.stringify(input) });
      onCreated();
    } catch (requestError) {
      onError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <Modal onClose={onClose} labelledBy="create-server-title">
      <form
        onSubmit={(event) => void submit(event)}
        className="mx-auto w-full max-w-lg border border-line bg-panel p-5 shadow-2xl sm:p-6"
      >
        <div className="flex items-start gap-4">
          <div className="grid size-9 place-items-center bg-signal text-ink">
            <Activity className="size-4" />
          </div>
          <div className="flex-1">
            <h2 id="create-server-title" className="text-lg font-semibold tracking-tight">
              Provision server
            </h2>
            <p className="mt-1 text-xs text-dim">Pull a GTNH image and create an isolated world.</p>
          </div>
          <button type="button" onClick={onClose} aria-label="Close">
            <X className="size-4" />
          </button>
        </div>
        <div className="mt-6 grid gap-4 sm:grid-cols-2">
          <label className="sm:col-span-2">
            <span className="mb-1.5 block text-[11px] font-medium">Server name</span>
            <input
              name="name"
              required
              maxLength={64}
              placeholder="Assembly Line"
              className="h-10 w-full border border-line bg-ink px-3 text-sm outline-none transition placeholder:text-dim/50 focus:border-signal"
            />
          </label>
          <label>
            <span className="mb-1.5 block text-[11px] font-medium">GTNH version</span>
            <input
              name="version"
              required
              defaultValue="stable-latest"
              className="h-10 w-full border border-line bg-ink px-3 font-mono text-xs outline-none focus:border-signal"
            />
          </label>
          <label>
            <span className="mb-1.5 block text-[11px] font-medium">Game port</span>
            <input
              name="port"
              type="number"
              min={1024}
              max={65535}
              required
              defaultValue={25565}
              className="h-10 w-full border border-line bg-ink px-3 font-mono text-xs outline-none focus:border-signal"
            />
          </label>
          <label className="sm:col-span-2">
            <span className="mb-1.5 block text-[11px] font-medium">Memory limit</span>
            <select
              name="memoryMb"
              defaultValue="6144"
              className="h-10 w-full border border-line bg-ink px-3 text-xs outline-none focus:border-signal"
            >
              <option value="4096">4 GB</option>
              <option value="6144">6 GB</option>
              <option value="8192">8 GB</option>
              <option value="12288">12 GB</option>
              <option value="16384">16 GB</option>
            </select>
          </label>
        </div>
        <div className="mt-6 flex justify-end gap-2 border-t border-line pt-4">
          <button type="button" onClick={onClose} className="h-9 border border-line px-4 text-xs text-dim hover:text-copy">
            Cancel
          </button>
          <button
            type="submit"
            disabled={submitting}
            className="inline-flex h-9 items-center gap-2 bg-signal px-4 text-xs font-semibold text-ink disabled:opacity-50"
          >
            {submitting && <LoaderCircle className="size-3.5 animate-spin" />} Create server
          </button>
        </div>
      </form>
    </Modal>
  );
}

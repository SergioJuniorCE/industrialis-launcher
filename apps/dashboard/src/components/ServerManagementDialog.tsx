import { useState, type FormEvent } from "react";
import type { GtnhServer, ServerConfigFile, ServerVersionDetail } from "@industrialis/server-contracts";
import useSWR from "swr";
import { CircleAlert, FilePenLine, LoaderCircle, RotateCw, Save, Server, Settings, X } from "lucide-react";
import { dashboardApi } from "../lib/dashboard-api.js";
import Modal from "./Modal.js";
import VersionPicker, { toVersionDetails } from "./VersionPicker.js";

type Section = "update" | "resources" | "files";

export default function ServerManagementDialog({
  server,
  versions,
  versionsLoading,
  versionsError,
  onClose,
  onUpdated,
}: {
  server: GtnhServer;
  versions: ServerVersionDetail[];
  versionsLoading: boolean;
  versionsError: string | null;
  onClose: () => void;
  onUpdated: () => void | Promise<void>;
}) {
  const [section, setSection] = useState<Section>("update");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [port, setPort] = useState(String(server.port));
  const [memoryMb, setMemoryMb] = useState(String(server.memoryMb));
  const [version, setVersion] = useState(server.version);
  const [createBackup, setCreateBackup] = useState(true);
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [fileDrafts, setFileDrafts] = useState<Record<string, string>>({});
  const versionsForServer =
    versions.some((detail) => detail.tag === server.version) || versionsLoading ? versions : [...toVersionDetails([server.version]), ...versions];
  const filesKey = section === "files" && server.status === "stopped" ? `/servers/${encodeURIComponent(server.id)}/files` : null;
  const { data: files = [], isLoading: filesLoading, error: filesError } = useSWR<string[]>(filesKey, dashboardApi);
  const fileQuery = selectedFile ? new URLSearchParams({ path: selectedFile }).toString() : "";
  const fileKey = section === "files" && selectedFile && server.status === "stopped" ? `/servers/${encodeURIComponent(server.id)}/file?${fileQuery}` : null;
  const { data: file, isLoading: fileLoading, error: fileError, mutate: refreshFile } = useSWR<ServerConfigFile>(fileKey, dashboardApi);

  const draft = selectedFile && file?.path === selectedFile ? (fileDrafts[selectedFile] ?? file.content) : "";

  async function updateVersion(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setPending(true);
    setError(null);
    let updated = false;
    try {
      await dashboardApi(`/servers/${encodeURIComponent(server.id)}/update`, {
        method: "POST",
        body: JSON.stringify({ version, createBackup }),
      });
      updated = true;
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setPending(false);
    }
    if (updated) await onUpdated();
  }

  async function updateResources(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setPending(true);
    setError(null);
    let updated = false;
    try {
      await dashboardApi(`/servers/${encodeURIComponent(server.id)}/resources`, {
        method: "PUT",
        body: JSON.stringify({ port: Number(port), memoryMb: Number(memoryMb) }),
      });
      updated = true;
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setPending(false);
    }
    if (updated) await onUpdated();
  }

  async function saveFile() {
    if (!selectedFile) return;
    setPending(true);
    setError(null);
    try {
      await dashboardApi(`/servers/${encodeURIComponent(server.id)}/file`, {
        method: "PUT",
        body: JSON.stringify({ path: selectedFile, content: draft }),
      });
      await refreshFile({ path: selectedFile, content: draft }, false);
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setPending(false);
    }
  }

  return (
    <Modal onClose={onClose} labelledBy="manage-server-title">
      <section className="mx-auto flex max-h-[90dvh] w-full max-w-3xl flex-col border border-line bg-panel shadow-2xl">
        <header className="flex items-center gap-3 border-b border-line px-5 py-4">
          <div className="grid size-9 place-items-center bg-signal text-ink">
            <Settings className="size-4" />
          </div>
          <div className="min-w-0 flex-1">
            <h2 id="manage-server-title" className="truncate text-base font-semibold">
              Manage {server.name}
            </h2>
            <p className="mt-1 truncate font-mono text-[10px] text-dim">
              {server.id} · {server.status} · {server.version}
            </p>
          </div>
          <button type="button" onClick={onClose} aria-label="Close server management">
            <X className="size-4" />
          </button>
        </header>

        <nav aria-label="Server management sections" className="flex border-b border-line px-3 pt-2">
          <SectionButton current={section} value="update" onClick={setSection} icon={<RotateCw className="size-3.5" />}>
            Update
          </SectionButton>
          <SectionButton current={section} value="resources" onClick={setSection} icon={<Server className="size-3.5" />}>
            Resources
          </SectionButton>
          <SectionButton current={section} value="files" onClick={setSection} icon={<FilePenLine className="size-3.5" />}>
            Config files
          </SectionButton>
        </nav>

        {error && <ErrorNotice message={error} onDismiss={() => setError(null)} />}

        {section === "update" && (
          <form onSubmit={(event) => void updateVersion(event)} className="min-h-0 overflow-y-auto p-5">
            <p className="text-sm font-medium">Installed release</p>
            <p className="mt-1 font-mono text-xs text-dim">{server.version}</p>
            <div className="mt-5">
              <span className="mb-1.5 block text-[11px] font-medium">New GTNH version</span>
              <VersionPicker versions={versionsForServer} value={version} onChange={setVersion} loading={versionsLoading} />
            </div>
            {versionsError && (
              <p role="alert" className="mt-2 text-xs text-danger">
                Could not refresh releases: {versionsError}
              </p>
            )}
            <label className="mt-4 flex items-start gap-2 text-xs text-copy">
              <input
                type="checkbox"
                checked={createBackup}
                onChange={(event) => setCreateBackup(event.currentTarget.checked)}
                className="mt-0.5 accent-signal"
              />
              <span>Create a pre-update backup of the server files and world. The update restores the previous container if it cannot start.</span>
            </label>
            <p className="mt-4 border-l-2 border-signal px-3 py-2 text-xs leading-5 text-dim">
              Updating stops this server, replaces its container, and starts it again if it was running. The named data volume and world directory are retained.
            </p>
            <div className="mt-6 flex justify-end border-t border-line pt-4">
              <button
                type="submit"
                disabled={pending || versionsLoading || version === server.version}
                className="inline-flex h-9 items-center gap-2 bg-signal px-4 text-xs font-semibold text-ink disabled:opacity-50"
              >
                {pending ? <LoaderCircle className="size-3.5 animate-spin" /> : <RotateCw className="size-3.5" />}
                Update server
              </button>
            </div>
          </form>
        )}

        {section === "resources" && (
          <form onSubmit={(event) => void updateResources(event)} className="min-h-0 overflow-y-auto p-5">
            <p className="text-sm font-medium">Server resources</p>
            <p className="mt-1 text-xs text-dim">Changes replace the container while keeping its world and server files.</p>
            <div className="mt-5 grid gap-4 sm:grid-cols-2">
              <label>
                <span className="mb-1.5 block text-[11px] font-medium">Game port</span>
                <input
                  name="port"
                  type="number"
                  min={1024}
                  max={65535}
                  required
                  value={port}
                  onChange={(event) => setPort(event.currentTarget.value)}
                  className="h-10 w-full border border-line bg-ink px-3 font-mono text-xs outline-none focus:border-signal"
                />
              </label>
              <label>
                <span className="mb-1.5 block text-[11px] font-medium">Memory limit (MB)</span>
                <input
                  name="memoryMb"
                  type="number"
                  min={4096}
                  max={131072}
                  step={1024}
                  required
                  value={memoryMb}
                  onChange={(event) => setMemoryMb(event.currentTarget.value)}
                  className="h-10 w-full border border-line bg-ink px-3 font-mono text-xs outline-none focus:border-signal"
                />
                <span className="mt-1 block font-mono text-[10px] text-dim">= {formatMemory(Number(memoryMb) || 0)}</span>
              </label>
            </div>
            <div className="mt-6 flex justify-end border-t border-line pt-4">
              <button
                type="submit"
                disabled={pending || (Number(port) === server.port && Number(memoryMb) === server.memoryMb)}
                className="h-9 bg-signal px-4 text-xs font-semibold text-ink disabled:opacity-50"
              >
                {pending ? <LoaderCircle className="inline size-3.5 animate-spin" /> : null} Save resources
              </button>
            </div>
          </form>
        )}

        {section === "files" && (
          <div className="grid min-h-0 flex-1 md:grid-cols-[220px_minmax(0,1fr)]">
            <aside className="max-h-48 overflow-y-auto border-b border-line p-3 md:max-h-none md:border-b-0 md:border-r">
              {server.status !== "stopped" ? (
                <p className="text-xs leading-5 text-dim">Stop this server to view and edit configuration files.</p>
              ) : filesLoading ? (
                <LoaderCircle className="mx-auto my-5 size-4 animate-spin text-signal" />
              ) : filesError ? (
                <p role="alert" className="text-xs text-danger">
                  {filesError.message}
                </p>
              ) : files.length === 0 ? (
                <p className="text-xs text-dim">No supported config files found.</p>
              ) : (
                <ul className="space-y-1">
                  {files.map((path) => (
                    <li key={path}>
                      <button
                        type="button"
                        onClick={() => setSelectedFile(path)}
                        aria-current={selectedFile === path ? "page" : undefined}
                        className={`w-full truncate px-2 py-2 text-left font-mono text-[10px] ${selectedFile === path ? "bg-signal-dark text-signal" : "text-dim hover:bg-panel-raised hover:text-copy"}`}
                      >
                        {path}
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </aside>
            <div className="flex min-h-64 min-w-0 flex-col p-3">
              {selectedFile ? (
                <>
                  <div className="mb-2 flex min-w-0 items-center justify-between gap-2">
                    <p className="truncate font-mono text-[10px] text-dim">{selectedFile}</p>
                    <button
                      type="button"
                      onClick={() => void saveFile()}
                      disabled={pending || fileLoading || server.status !== "stopped" || file?.path !== selectedFile}
                      className="inline-flex h-8 shrink-0 items-center gap-1.5 bg-signal px-3 text-[10px] font-semibold text-ink disabled:opacity-50"
                    >
                      {pending ? <LoaderCircle className="size-3 animate-spin" /> : <Save className="size-3" />}
                      Save file
                    </button>
                  </div>
                  {fileError && (
                    <p role="alert" className="mb-2 text-xs text-danger">
                      {fileError.message}
                    </p>
                  )}
                  <textarea
                    aria-label={`${selectedFile} contents`}
                    value={draft}
                    onChange={(event) => {
                      const value = event.currentTarget.value;
                      setFileDrafts((current) => ({ ...current, [selectedFile]: value }));
                    }}
                    readOnly={server.status !== "stopped" || fileLoading || file?.path !== selectedFile}
                    spellCheck={false}
                    className="min-h-0 flex-1 resize-none border border-line bg-ink p-3 font-mono text-[11px] leading-5 text-copy outline-none focus:border-signal"
                  />
                </>
              ) : (
                <p className="m-auto text-xs text-dim">Choose a config file to view it.</p>
              )}
            </div>
          </div>
        )}
      </section>
    </Modal>
  );
}

function SectionButton({
  current,
  value,
  onClick,
  icon,
  children,
}: {
  current: Section;
  value: Section;
  onClick: (section: Section) => void;
  icon: React.ReactNode;
  children: string;
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={current === value}
      onClick={() => onClick(value)}
      className={`inline-flex h-9 items-center gap-2 border-b-2 px-3 text-[11px] ${current === value ? "border-signal text-signal" : "border-transparent text-dim hover:text-copy"}`}
    >
      {icon}
      {children}
    </button>
  );
}

function ErrorNotice({ message, onDismiss }: { message: string; onDismiss: () => void }) {
  return (
    <div role="alert" className="flex items-start gap-2 border-b border-danger/30 bg-danger/5 px-4 py-3 text-xs text-danger">
      <CircleAlert className="mt-0.5 size-4 shrink-0" />
      <p className="flex-1">{message}</p>
      <button type="button" onClick={onDismiss} aria-label="Dismiss management error">
        <X className="size-3.5" />
      </button>
    </div>
  );
}

function formatMemory(memoryMb: number): string {
  return `${Number.isInteger(memoryMb / 1024) ? memoryMb / 1024 : (memoryMb / 1024).toFixed(1)} GB`;
}

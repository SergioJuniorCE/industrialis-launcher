import { useCallback, useEffect, useRef, useState } from "react";
import { CircleAlert, Plus, Star, Trash2, UserPlus, WifiOff } from "lucide-react";
import { invoke, isDesktop } from "../lib/desktop";
import { useLauncherStore, type LauncherAccount } from "../stores/launcher-store";
import { SkinFace } from "./AccountSwitcher";
import { MicrosoftAccountDialog } from "./MicrosoftAccountDialog";
import { Badge } from "./ui/badge";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Label } from "./ui/label";

const OFFLINE_USERNAME_RE = /^[a-zA-Z0-9_]{1,16}$/;

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function accountLabel(account: LauncherAccount): string {
  if (account.username.trim()) return account.username;
  return account.account_type === "offline" ? "Offline account" : "Microsoft account";
}

function microsoftLicenseStatus(account: LauncherAccount): string | null {
  if (account.account_type === "offline") return null;
  if (account.can_play_minecraft === false) {
    return "Does not own Minecraft Java Edition";
  }
  if (account.owns_minecraft === false && account.can_play_minecraft) {
    return "Plays via PC Game Pass";
  }
  return null;
}

export function AccountsTab({
  onSetDefaultAccount,
  defaultAccountId,
  launchRedirect,
  onDismissRedirect,
}: {
  onSetDefaultAccount: (id: string | null) => void;
  defaultAccountId: string | null;
  launchRedirect?: { instanceName: string } | null;
  onDismissRedirect?: () => void;
}) {
  const accounts = useLauncherStore((state) => state.accounts);
  const setAccounts = useLauncherStore((state) => state.setAccounts);
  const [microsoftDialogOpen, setMicrosoftDialogOpen] = useState(false);
  const [accountLoadError, setAccountLoadError] = useState<string | null>(null);
  const [offlineError, setOfflineError] = useState<string | null>(null);
  const [offlineUsername, setOfflineUsername] = useState("");
  const [addingOffline, setAddingOffline] = useState(false);
  const addingOfflineRef = useRef(false);

  const load = useCallback(async () => {
    if (!isDesktop()) return;
    setAccountLoadError(null);
    try {
      setAccounts(await invoke<LauncherAccount[]>("get_accounts"));
    } catch (error) {
      setAccountLoadError(errorMessage(error));
    }
  }, [setAccounts]);

  useEffect(() => {
    void load();
  }, [load]);

  const handleMicrosoftAccountAdded = useCallback(
    (account: LauncherAccount) => {
      onSetDefaultAccount(account.id);
      void load();
      onDismissRedirect?.();
    },
    [load, onDismissRedirect, onSetDefaultAccount],
  );

  const handleAddOffline = async () => {
    if (addingOfflineRef.current) return;
    const trimmed = offlineUsername.trim();
    if (!OFFLINE_USERNAME_RE.test(trimmed)) {
      setOfflineError("Use 1-16 letters, numbers, or underscores.");
      return;
    }
    addingOfflineRef.current = true;
    setAddingOffline(true);
    setOfflineError(null);
    try {
      const account = await invoke<LauncherAccount>("add_offline_account", { username: trimmed });
      setOfflineUsername("");
      onSetDefaultAccount(account.id);
      void load();
      onDismissRedirect?.();
    } catch (error) {
      setOfflineError(`${error}`);
    } finally {
      addingOfflineRef.current = false;
      setAddingOffline(false);
    }
  };

  const handleRemove = async (id: string) => {
    await invoke("remove_account", { id });
    if (defaultAccountId === id) {
      onSetDefaultAccount(null);
    }
    void load();
  };

  return (
    <div className="space-y-5 pb-4">
      <div className="flex flex-col gap-3 border-b border-border/70 pb-4 sm:flex-row sm:items-start sm:justify-between">
        <div>
          <h1 className="text-lg font-semibold tracking-tight">Accounts</h1>
          <p className="mt-1 max-w-xl text-xs leading-relaxed text-muted-foreground">Choose which accounts Industrialis can use to launch Minecraft.</p>
        </div>
        <Button type="button" size="sm" className="shrink-0 gap-1.5" onClick={() => setMicrosoftDialogOpen(true)}>
          <Plus className="size-3.5" />
          Add Microsoft
        </Button>
      </div>

      {launchRedirect && (
        <div className="flex items-start gap-2.5 rounded-lg border border-amber-500/40 bg-amber-500/8 px-3.5 py-3 text-xs" role="alert">
          <CircleAlert className="mt-0.5 size-4 shrink-0 text-amber-500" />
          <div>
            <p className="font-medium">Choose an account before launching {launchRedirect.instanceName}</p>
            <p className="mt-0.5 text-muted-foreground">Set an account as default with the star button, then return to Instances.</p>
          </div>
        </div>
      )}

      <section className="overflow-hidden rounded-lg border border-border/80 bg-card/50 shadow-sm" aria-labelledby="linked-accounts-title">
        <div className="flex items-center justify-between gap-3 border-b border-border/70 px-4 py-3.5">
          <div>
            <h2 id="linked-accounts-title" className="text-sm font-semibold">
              Linked accounts
            </h2>
            <p className="mt-0.5 text-xs text-muted-foreground">
              {accounts.length === 0 ? "No accounts connected yet." : `${accounts.length} account${accounts.length === 1 ? "" : "s"} available to launch.`}
            </p>
          </div>
          <Badge variant="secondary" className="shrink-0 text-[10px]">
            {accounts.length}
          </Badge>
        </div>

        {accountLoadError && (
          <div className="flex items-start gap-2.5 border-b border-destructive/25 bg-destructive/8 px-4 py-3 text-xs" role="alert">
            <CircleAlert className="mt-0.5 size-4 shrink-0 text-destructive" />
            <div>
              <p className="font-medium">Could not load accounts</p>
              <p className="mt-0.5 text-muted-foreground">{accountLoadError}</p>
            </div>
          </div>
        )}

        {accounts.length > 0 ? (
          <div className="space-y-1.5 p-2">
            {accounts.map((account) => {
              const isDefault = account.id === defaultAccountId;
              const licenseStatus = microsoftLicenseStatus(account);
              return (
                <div
                  key={account.id}
                  className={`flex items-center gap-3 rounded-md border px-3 py-2.5 transition-colors ${
                    isDefault ? "border-primary/35 bg-primary/8" : "border-transparent hover:border-border/70 hover:bg-muted/35"
                  }`}
                >
                  {account.skin_png_base64 ? (
                    <SkinFace skin={account.skin_png_base64} className="size-9" />
                  ) : (
                    <div className="flex size-9 shrink-0 items-center justify-center rounded-md bg-secondary text-sm font-semibold">
                      {account.username.charAt(0).toUpperCase() || "?"}
                    </div>
                  )}
                  <div className="min-w-0 flex-1">
                    <div className="flex min-w-0 flex-wrap items-center gap-1.5">
                      <span className="truncate text-sm font-medium">{accountLabel(account)}</span>
                      <Badge variant="secondary" className="h-5 shrink-0 px-1.5 text-[10px] capitalize">
                        {account.account_type === "offline" ? "Offline" : "Microsoft"}
                      </Badge>
                      {isDefault && <span className="text-[10px] font-medium text-primary">Default</span>}
                    </div>
                    <p className={`mt-0.5 truncate text-[11px] ${account.can_play_minecraft === false ? "text-amber-500" : "text-muted-foreground"}`}>
                      {licenseStatus ?? (account.account_type === "offline" ? "Offline profile" : "Minecraft account connected")}
                    </p>
                  </div>
                  <div className="flex shrink-0 items-center gap-1">
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon"
                      className="size-8"
                      title={isDefault ? `${accountLabel(account)} is the default account` : `Set ${accountLabel(account)} as default`}
                      aria-label={isDefault ? `${accountLabel(account)} is the default account` : `Set ${accountLabel(account)} as default`}
                      aria-pressed={isDefault}
                      onClick={() => {
                        if (!isDefault) onSetDefaultAccount(account.id);
                      }}
                    >
                      <Star className={`size-4 ${isDefault ? "fill-amber-400 text-amber-400" : "text-muted-foreground hover:text-foreground"}`} />
                    </Button>
                    <Button
                      type="button"
                      size="icon"
                      variant="ghost"
                      className="size-8 text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                      aria-label={`Remove ${accountLabel(account)}`}
                      title={`Remove ${accountLabel(account)}`}
                      onClick={() => void handleRemove(account.id)}
                    >
                      <Trash2 className="size-4" />
                    </Button>
                  </div>
                </div>
              );
            })}
          </div>
        ) : (
          <div className="flex items-center gap-3 px-4 py-5">
            <div className="flex size-9 shrink-0 items-center justify-center rounded-md bg-secondary text-muted-foreground">
              <UserPlus className="size-4" />
            </div>
            <div className="min-w-0">
              <p className="text-sm font-medium">No accounts connected</p>
              <p className="mt-0.5 text-xs leading-relaxed text-muted-foreground">Add a Microsoft account above, or create an offline profile below.</p>
            </div>
          </div>
        )}
      </section>

      <section className="rounded-lg border border-border/80 bg-card/35 p-4" aria-labelledby="offline-account-title">
        <div className="flex items-start gap-3">
          <div className="flex size-9 shrink-0 items-center justify-center rounded-md bg-secondary text-muted-foreground">
            <WifiOff className="size-4" />
          </div>
          <div className="min-w-0 flex-1">
            <h2 id="offline-account-title" className="text-sm font-semibold">
              Offline profile
            </h2>
            <p className="mt-0.5 text-xs leading-relaxed text-muted-foreground">Play without signing in. Use 1-16 letters, numbers, or underscores.</p>
            <div className="mt-3 grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-end">
              <div className="space-y-1.5">
                <Label htmlFor="offline-username" className="text-xs">
                  Username
                </Label>
                <Input
                  id="offline-username"
                  value={offlineUsername}
                  onChange={(event) => {
                    setOfflineUsername(event.target.value);
                    if (offlineError) setOfflineError(null);
                  }}
                  placeholder="Steve"
                  maxLength={16}
                  className="h-9 font-mono text-xs"
                  aria-invalid={Boolean(offlineError)}
                  onKeyDown={(event) => {
                    if (event.key !== "Enter" || addingOffline || !offlineUsername.trim()) return;
                    void handleAddOffline();
                  }}
                />
              </div>
              <Button type="button" size="sm" variant="outline" disabled={addingOffline || !offlineUsername.trim()} onClick={() => void handleAddOffline()}>
                {addingOffline ? "Creating..." : "Create profile"}
              </Button>
            </div>
            {offlineError && (
              <p className="mt-2 text-xs text-destructive" role="alert">
                {offlineError}
              </p>
            )}
          </div>
        </div>
      </section>

      <MicrosoftAccountDialog open={microsoftDialogOpen} onOpenChange={setMicrosoftDialogOpen} onAccountAdded={handleMicrosoftAccountAdded} />
    </div>
  );
}

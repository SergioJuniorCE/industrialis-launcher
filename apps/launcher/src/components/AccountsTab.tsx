import { useCallback, useEffect, useState } from "react";
import { invoke, listen } from "../lib/desktop";
import { Check, Copy, Star } from "lucide-react";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Badge } from "./ui/badge";

import { SkinFace } from "./AccountSwitcher";
import { useLauncherStore, type LauncherAccount } from "../stores/launcher-store";

interface DeviceCodeInfo {
  user_code: string;
  verification_uri: string;
  message: string;
}

const OFFLINE_USERNAME_RE = /^[a-zA-Z0-9_]{1,16}$/;

async function copyText(text: string): Promise<void> {
  if (!navigator.clipboard?.writeText) {
    throw new Error("Clipboard access is not available");
  }
  await navigator.clipboard.writeText(text);
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
  const [loggingIn, setLoggingIn] = useState(false);
  const [deviceCode, setDeviceCode] = useState<DeviceCodeInfo | null>(null);
  const [deviceCodeCopied, setDeviceCodeCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [offlineUsername, setOfflineUsername] = useState("");
  const [addingOffline, setAddingOffline] = useState(false);

  const load = useCallback(() => {
    invoke<LauncherAccount[]>("get_accounts")
      .then(setAccounts)
      .catch(() => {});
  }, [setAccounts]);
  useEffect(() => load(), [load]);

  useEffect(() => {
    const unlisten = listen<DeviceCodeInfo>("auth-device-code", (e) => {
      setDeviceCode(e.payload);
      setDeviceCodeCopied(false);
      void copyText(e.payload.user_code)
        .then(() => setDeviceCodeCopied(true))
        .catch(() => {
          // Clipboard permissions can reject automatic writes. The visible
          // copy button still lets the user copy from an explicit action.
        });
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  const handleCopyDeviceCode = async () => {
    if (!deviceCode) return;
    try {
      await copyText(deviceCode.user_code);
      setDeviceCodeCopied(true);
    } catch {
      setDeviceCodeCopied(false);
    }
  };

  const handleMicrosoftLogin = async () => {
    setLoggingIn(true);
    setError(null);
    setDeviceCode(null);
    setDeviceCodeCopied(false);
    try {
      const account = await invoke<LauncherAccount>("start_microsoft_login");
      onSetDefaultAccount(account.id);
      load();
    } catch (e) {
      setError(`${e}`);
    }
    setLoggingIn(false);
    setDeviceCode(null);
    setDeviceCodeCopied(false);
  };

  const handleAddOffline = async () => {
    const trimmed = offlineUsername.trim();
    if (!OFFLINE_USERNAME_RE.test(trimmed)) {
      setError("Username must be 1-16 characters: letters, numbers, and underscores only.");
      return;
    }
    setAddingOffline(true);
    setError(null);
    try {
      const account = await invoke<LauncherAccount>("add_offline_account", { username: trimmed });
      setOfflineUsername("");
      onSetDefaultAccount(account.id);
      load();
      onDismissRedirect?.();
    } catch (e) {
      setError(`${e}`);
    } finally {
      setAddingOffline(false);
    }
  };

  const handleRemove = async (id: string) => {
    await invoke("remove_account", { id });
    if (defaultAccountId === id) {
      onSetDefaultAccount(null);
    }
    load();
  };

  return (
    <div className="space-y-4">
      <div>
        <h2 className="text-sm font-semibold">Accounts</h2>
        <p className="text-xs text-muted-foreground mt-0.5">
          Click the star on an account to use it for every launch. Override per instance in instance settings if needed.
        </p>
      </div>

      {launchRedirect && (
        <div className="rounded-md border border-amber-500/40 bg-amber-500/5 px-3 py-2 text-xs">
          <p className="font-medium">Default account required to launch {launchRedirect.instanceName}</p>
          <p className="text-muted-foreground mt-0.5">Add an account below and click its star, then return to Instances.</p>
        </div>
      )}

      {accounts.length > 0 && (
        <div className="rounded-md border border-border divide-y">
          {accounts.map((acc) => {
            const isDefault = acc.id === defaultAccountId;
            const licenseStatus = microsoftLicenseStatus(acc);
            return (
              <div key={acc.id} className={`flex items-center gap-2 px-2.5 py-1.5 ${isDefault ? "bg-muted/50" : "hover:bg-muted/30"}`}>
                {acc.skin_png_base64 ? (
                  <SkinFace skin={acc.skin_png_base64} className="size-6" />
                ) : (
                  <div className="size-6 rounded-sm bg-secondary flex items-center justify-center text-[10px] font-medium shrink-0">
                    {acc.username.charAt(0).toUpperCase() || "?"}
                  </div>
                )}
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-1.5 min-w-0">
                    <span className="text-xs font-medium truncate">{acc.username || "(no username)"}</span>
                    <Badge variant="secondary" className="text-[10px] h-4 px-1 capitalize shrink-0">
                      {acc.account_type === "offline" ? "Offline" : "Microsoft"}
                    </Badge>
                  </div>
                  {acc.uuid && <p className="font-mono text-[10px] text-muted-foreground truncate">{acc.uuid}</p>}
                  {licenseStatus && (
                    <p className={`text-[10px] ${acc.can_play_minecraft === false ? "text-amber-500" : "text-muted-foreground"}`}>{licenseStatus}</p>
                  )}
                </div>
                <div className="flex items-center gap-0.5 shrink-0">
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    className="size-6 shrink-0"
                    title={isDefault ? `${accountLabel(acc)} is the default account` : `Set ${accountLabel(acc)} as default`}
                    aria-label={isDefault ? `${accountLabel(acc)} is the default account` : `Set ${accountLabel(acc)} as default`}
                    aria-pressed={isDefault}
                    onClick={() => {
                      if (!isDefault) onSetDefaultAccount(acc.id);
                    }}
                  >
                    <Star className={`size-3.5 ${isDefault ? "fill-amber-400 text-amber-400" : "text-muted-foreground hover:text-foreground"}`} />
                  </Button>
                  <Button size="sm" variant="destructive" className="h-6 px-2" onClick={() => void handleRemove(acc.id)}>
                    Remove
                  </Button>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {accounts.length === 0 && !launchRedirect && <p className="text-muted-foreground text-xs">No accounts yet.</p>}

      <div className="rounded-md border border-border p-3 space-y-2">
        <div>
          <p className="text-xs font-medium">Microsoft account</p>
          <p className="text-[11px] text-muted-foreground">
            Sign in with your Microsoft account to play online. Ownership is verified via the Mojang API after login.
          </p>
        </div>
        <Button size="sm" className="w-full" onClick={() => void handleMicrosoftLogin()} disabled={loggingIn}>
          {loggingIn ? "Logging in…" : "Sign in with Microsoft"}
        </Button>
      </div>

      <div className="rounded-md border border-border p-3 space-y-2">
        <div>
          <p className="text-xs font-medium">Offline account</p>
          <p className="text-[11px] text-muted-foreground">Play without signing in. Letters, numbers, underscores (up to 16 characters).</p>
        </div>
        <div className="flex gap-2">
          <Input
            value={offlineUsername}
            onChange={(e) => setOfflineUsername(e.target.value)}
            placeholder="Steve"
            maxLength={16}
            className="font-mono h-8 text-xs flex-1"
            onKeyDown={(e) => e.key === "Enter" && void handleAddOffline()}
          />
          <Button size="sm" variant="outline" disabled={addingOffline || !offlineUsername.trim()} onClick={() => void handleAddOffline()}>
            {addingOffline ? "Creating…" : "Create"}
          </Button>
        </div>
      </div>

      {loggingIn && deviceCode && (
        <div className="rounded-md border border-border p-3 space-y-2">
          <p className="text-xs font-medium">Complete Microsoft sign-in</p>
          <p className="text-[11px] text-muted-foreground">
            Paste the code below at{" "}
            <a className="text-foreground underline" href={deviceCode.verification_uri} target="_blank" rel="noreferrer">
              {deviceCode.verification_uri}
            </a>
          </p>
          <div className="flex items-center gap-2">
            <div
              className="flex min-w-0 flex-1 items-center justify-center rounded-md border border-border bg-muted/40 px-3 py-2"
              aria-label={`Microsoft device code ${deviceCode.user_code}`}
            >
              <span className="select-all text-xl font-mono font-semibold tracking-widest">{deviceCode.user_code}</span>
            </div>
            <Button type="button" size="sm" variant="outline" className="h-10 shrink-0 gap-1.5" onClick={() => void handleCopyDeviceCode()}>
              {deviceCodeCopied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
              {deviceCodeCopied ? "Copied" : "Copy code"}
            </Button>
          </div>
          <p className="text-[10px] text-muted-foreground">
            {deviceCodeCopied
              ? "The code was copied automatically. Paste it into the Microsoft page."
              : "If it was not copied automatically, use Copy code or select it manually."}
          </p>
          <p className="text-[10px] text-muted-foreground">{deviceCode.message}</p>
        </div>
      )}

      {error && <p className="text-destructive text-xs">{error}</p>}
    </div>
  );
}

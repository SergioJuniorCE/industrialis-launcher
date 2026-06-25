import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Star } from "lucide-react";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Badge } from "./ui/badge";

import type { LauncherAccount } from "./AccountSwitcher";

interface DeviceCodeInfo {
  user_code: string;
  verification_uri: string;
  message: string;
}

const OFFLINE_USERNAME_RE = /^[a-zA-Z0-9_]{1,16}$/;

function accountLabel(account: LauncherAccount): string {
  if (account.username.trim()) return account.username;
  return account.account_type === "offline" ? "Offline account" : "Microsoft account";
}

export function AccountsTab({
  onAccountsChanged,
  onSetDefaultAccount,
  defaultAccountId,
  launchRedirect,
  onDismissRedirect,
}: {
  onAccountsChanged?: () => void;
  onSetDefaultAccount: (id: string | null) => void;
  defaultAccountId: string | null;
  launchRedirect?: { instanceName: string } | null;
  onDismissRedirect?: () => void;
}) {
  const [accounts, setAccounts] = useState<LauncherAccount[]>([]);
  const [loggingIn, setLoggingIn] = useState(false);
  const [deviceCode, setDeviceCode] = useState<DeviceCodeInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [offlineUsername, setOfflineUsername] = useState("");
  const [addingOffline, setAddingOffline] = useState(false);

  const load = () => {
    invoke<LauncherAccount[]>("get_accounts")
      .then((list) => {
        setAccounts(list);
        onAccountsChanged?.();
      })
      .catch(() => {});
  };
  useEffect(load, []);

  useEffect(() => {
    const unlisten = listen<DeviceCodeInfo>("auth-device-code", (e) => {
      setDeviceCode(e.payload);
    });
    return () => { unlisten.then((f) => f()); };
  }, []);

  const handleMicrosoftLogin = async () => {
    setLoggingIn(true);
    setError(null);
    setDeviceCode(null);
    try {
      const account = await invoke<LauncherAccount>("start_microsoft_login");
      onSetDefaultAccount(account.id);
      load();
    } catch (e) {
      setError(`${e}`);
    }
    setLoggingIn(false);
    setDeviceCode(null);
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
          <p className="text-muted-foreground mt-0.5">
            Add an account below and click its star, then return to Instances.
          </p>
        </div>
      )}

      {accounts.length > 0 && (
        <div className="rounded-md border border-border divide-y">
          {accounts.map((acc) => {
            const isDefault = acc.id === defaultAccountId;
            return (
            <div
              key={acc.id}
              className={`flex items-center gap-2 px-2.5 py-1.5 ${
                isDefault ? "bg-muted/50" : "hover:bg-muted/30"
              }`}
            >
              {acc.skin_png_base64 ? (
                <img
                  src={`data:image/png;base64,${acc.skin_png_base64}`}
                  alt=""
                  className="size-6 rounded-sm shrink-0 image-pixelated"
                  style={{ imageRendering: "pixelated" }}
                />
              ) : (
                <div className="size-6 rounded-sm bg-secondary flex items-center justify-center text-[10px] font-medium shrink-0">
                  {acc.username.charAt(0).toUpperCase() || "?"}
                </div>
              )}
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5 min-w-0">
                  <span className="text-xs font-medium truncate">
                    {acc.username || "(no username)"}
                  </span>
                  <Badge variant="secondary" className="text-[10px] h-4 px-1 capitalize shrink-0">
                    {acc.account_type === "offline" ? "Offline" : "Microsoft"}
                  </Badge>
                </div>
                {acc.uuid && (
                  <p className="font-mono text-[10px] text-muted-foreground truncate">{acc.uuid}</p>
                )}
                {acc.account_type !== "offline" && acc.owns_minecraft === false && (
                  <p className="text-[10px] text-amber-500">Does not own Minecraft Java Edition</p>
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
                  <Star
                    className={`size-3.5 ${
                      isDefault ? "fill-amber-400 text-amber-400" : "text-muted-foreground hover:text-foreground"
                    }`}
                  />
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

      {accounts.length === 0 && !launchRedirect && (
        <p className="text-muted-foreground text-xs">No accounts yet.</p>
      )}

      <div className="rounded-md border border-border p-3 space-y-2">
        <div>
          <p className="text-xs font-medium">Offline account</p>
          <p className="text-[11px] text-muted-foreground">
            Letters, numbers, underscores (up to 16 characters).
          </p>
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
          <Button
            size="sm"
            disabled={addingOffline || !offlineUsername.trim()}
            onClick={() => void handleAddOffline()}
          >
            {addingOffline ? "Creating…" : "Create"}
          </Button>
        </div>
        <Button
          size="sm"
          variant="outline"
          className="w-full"
          onClick={() => void handleMicrosoftLogin()}
          disabled={loggingIn}
        >
          {loggingIn ? "Logging in…" : "Add Microsoft account"}
        </Button>
      </div>

      {loggingIn && deviceCode && (
        <div className="rounded-md border border-border p-3 space-y-1.5">
          <p className="text-xs font-medium">Device code login</p>
          <p className="text-[11px] text-muted-foreground">
            Enter this code at{" "}
            <a className="text-foreground underline" href={deviceCode.verification_uri} target="_blank" rel="noreferrer">
              {deviceCode.verification_uri}
            </a>
          </p>
          <p className="text-xl font-mono tracking-widest">{deviceCode.user_code}</p>
          <p className="text-[10px] text-muted-foreground">{deviceCode.message}</p>
        </div>
      )}

      {error && <p className="text-destructive text-xs">{error}</p>}
    </div>
  );
}
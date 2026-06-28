import { useEffect, useRef, useState } from "react";
import { ChevronDown, User, UserCircle } from "lucide-react";
import { Button } from "./ui/button";
import { Badge } from "./ui/badge";

export interface LauncherAccount {
  id: string;
  username: string;
  uuid: string;
  account_type: string;
  skin_png_base64?: string;
  owns_minecraft?: boolean;
  can_play_minecraft?: boolean;
}

function accountLabel(account: LauncherAccount): string {
  if (account.username.trim()) return account.username;
  return account.account_type === "offline" ? "Offline account" : "Microsoft account";
}

export function AccountSwitcher({
  accounts,
  defaultAccountId,
  onSelectDefaultAccount,
  onManageAccounts,
}: {
  accounts: LauncherAccount[];
  defaultAccountId: string | null;
  onSelectDefaultAccount: (id: string) => void;
  onManageAccounts: () => void;
}) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const selected = accounts.find((a) => a.id === defaultAccountId) ?? null;

  useEffect(() => {
    if (!open) return;
    const handleClick = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open]);

  return (
    <div ref={containerRef} className="relative">
      <Button
        variant="ghost"
        size="sm"
        className="gap-1.5 max-w-[180px]"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        title={
          selected
            ? `Default account: ${accountLabel(selected)}`
            : "Set default account for launch"
        }
      >
        {selected?.skin_png_base64 ? (
          <img
            src={`data:image/png;base64,${selected.skin_png_base64}`}
            alt=""
            className="size-5 rounded-sm shrink-0 image-pixelated"
            style={{ imageRendering: "pixelated" }}
          />
        ) : (
          <UserCircle className="size-4 shrink-0 text-muted-foreground" />
        )}
        <span className="text-xs truncate">
          {selected ? accountLabel(selected) : "No default"}
        </span>
        <ChevronDown className="size-3 shrink-0 text-muted-foreground" />
      </Button>

      {open && (
        <div className="absolute right-0 top-full z-50 mt-1 w-64 rounded-md border border-border bg-popover p-1 shadow-lg">
          {accounts.length === 0 ? (
            <div className="px-3 py-2 text-xs text-muted-foreground">
              No accounts yet. Add one to launch instances.
            </div>
          ) : (
            <div className="max-h-64 overflow-y-auto">
              {accounts.map((acc) => (
                <Button
                  key={acc.id}
                  type="button"
                  variant="ghost"
                  className={`h-auto w-full justify-start gap-2 px-2 py-1.5 text-xs font-normal ${
                    acc.id === defaultAccountId ? "bg-muted" : ""
                  }`}
                  onClick={() => {
                    onSelectDefaultAccount(acc.id);
                    setOpen(false);
                  }}
                >
                  {acc.skin_png_base64 ? (
                    <img
                      src={`data:image/png;base64,${acc.skin_png_base64}`}
                      alt=""
                      className="size-6 rounded-sm shrink-0 image-pixelated"
                      style={{ imageRendering: "pixelated" }}
                    />
                  ) : (
                    <User className="size-4 shrink-0 text-muted-foreground" />
                  )}
                  <div className="min-w-0 flex-1">
                    <div className="truncate font-medium">{accountLabel(acc)}</div>
                    <div className="text-[10px] text-muted-foreground capitalize">
                      {acc.account_type === "offline" ? "Offline" : "Microsoft"}
                    </div>
                  </div>
                  {acc.id === defaultAccountId && (
                    <Badge variant="secondary" className="text-[10px] h-4 px-1 shrink-0">
                      Default
                    </Badge>
                  )}
                </Button>
              ))}
            </div>
          )}
          <div className="border-t border-border mt-1 pt-1">
            <Button
              variant="ghost"
              size="sm"
              className="w-full justify-start text-xs"
              onClick={() => {
                setOpen(false);
                onManageAccounts();
              }}
            >
              Manage accounts…
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
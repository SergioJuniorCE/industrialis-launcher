import { useCallback, useEffect, useRef, useState } from "react";
import { Check, CircleAlert, Copy, ExternalLink, LoaderCircle, ShieldCheck, X } from "lucide-react";
import { invoke, listen, openUrl } from "../lib/desktop";
import type { LauncherAccount } from "../stores/launcher-store";
import { Button } from "./ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "./ui/dialog";

export interface DeviceCodeInfo {
  user_code: string;
  verification_uri: string;
  message: string;
}

type LoginPhase = "starting" | "waiting" | "error";

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

async function copyText(text: string): Promise<void> {
  if (!navigator.clipboard?.writeText) {
    throw new Error("Clipboard access is not available");
  }
  await navigator.clipboard.writeText(text);
}

function MicrosoftMark() {
  return (
    <span className="grid size-9 shrink-0 grid-cols-2 gap-0.5 rounded-md bg-background p-1.5 shadow-sm" aria-hidden="true">
      <span className="bg-[#f25022]" />
      <span className="bg-[#7fba00]" />
      <span className="bg-[#00a4ef]" />
      <span className="bg-[#ffb900]" />
    </span>
  );
}

export function MicrosoftAccountDialog({
  open,
  onOpenChange,
  onAccountAdded,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onAccountAdded: (account: LauncherAccount) => void;
}) {
  const [phase, setPhase] = useState<LoginPhase>("starting");
  const [deviceCode, setDeviceCode] = useState<DeviceCodeInfo | null>(null);
  const [deviceCodeCopied, setDeviceCodeCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const startedForOpenRef = useRef(false);

  const reset = useCallback(() => {
    setPhase("starting");
    setDeviceCode(null);
    setDeviceCodeCopied(false);
    setError(null);
  }, []);

  const startLogin = useCallback(async () => {
    setPhase("starting");
    setDeviceCode(null);
    setDeviceCodeCopied(false);
    setError(null);

    try {
      const account = await invoke<LauncherAccount>("start_microsoft_login");
      onAccountAdded(account);
      onOpenChange(false);
    } catch (loginError) {
      setError(errorMessage(loginError));
      setPhase("error");
    }
  }, [onAccountAdded, onOpenChange]);

  useEffect(() => {
    if (!open) {
      startedForOpenRef.current = false;
      reset();
      return;
    }

    const unlisten = listen<DeviceCodeInfo>("auth-device-code", (event) => {
      setDeviceCode(event.payload);
      setDeviceCodeCopied(false);
      setPhase("waiting");
      void copyText(event.payload.user_code)
        .then(() => setDeviceCodeCopied(true))
        .catch(() => {
          // The explicit copy button and selectable text remain available when
          // the browser denies automatic clipboard access.
        });
    });

    return () => {
      void unlisten.then((unsubscribe) => unsubscribe());
    };
  }, [open, reset]);

  useEffect(() => {
    if (!open || startedForOpenRef.current) return;
    startedForOpenRef.current = true;
    void startLogin();
  }, [open, startLogin]);

  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen) reset();
    onOpenChange(nextOpen);
  };

  const handleCopy = async () => {
    if (!deviceCode) return;
    setError(null);
    try {
      await copyText(deviceCode.user_code);
      setDeviceCodeCopied(true);
    } catch (copyError) {
      setDeviceCodeCopied(false);
      setError(errorMessage(copyError));
    }
  };

  const handleOpenSignIn = async () => {
    if (!deviceCode) return;
    setError(null);
    try {
      await openUrl(deviceCode.verification_uri);
    } catch (openError) {
      setError(errorMessage(openError));
    }
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="w-[min(92vw,34rem)] max-w-lg overflow-hidden p-0">
        <div className="border-b border-border/80 bg-muted/20 px-5 py-4">
          <div className="flex items-start gap-3">
            <MicrosoftMark />
            <div className="min-w-0 flex-1">
              <DialogTitle className="text-base">Add Microsoft account</DialogTitle>
              <DialogDescription className="mt-1 text-xs leading-relaxed">Connect the Microsoft account that owns Minecraft Java Edition.</DialogDescription>
            </div>
            <Button type="button" variant="ghost" size="icon" className="size-7 shrink-0" aria-label="Close" onClick={() => handleOpenChange(false)}>
              <X className="size-3.5" />
            </Button>
          </div>
        </div>

        <div className="space-y-4 px-5 py-5">
          {phase === "starting" && (
            <div className="space-y-4">
              <div className="flex items-start gap-3 rounded-lg border border-primary/30 bg-primary/8 px-3.5 py-3.5">
                <LoaderCircle className="mt-0.5 size-4 shrink-0 animate-spin text-primary motion-reduce:animate-none" />
                <div>
                  <p className="text-sm font-medium">Opening Microsoft sign-in</p>
                  <p className="mt-1 text-xs leading-relaxed text-muted-foreground">
                    Finish signing in through your browser. This window will update when your account is ready.
                  </p>
                </div>
              </div>

              <div className="flex items-start gap-2.5 rounded-lg border border-border/70 bg-background/30 px-3.5 py-3 text-xs text-muted-foreground">
                <ShieldCheck className="mt-0.5 size-4 shrink-0 text-primary" />
                <p>Industrialis never asks for your Microsoft password in the launcher.</p>
              </div>
            </div>
          )}

          {phase === "waiting" && deviceCode && (
            <div className="space-y-4">
              <div>
                <p className="text-sm font-medium">Finish in your browser</p>
                <p className="mt-1 text-xs leading-relaxed text-muted-foreground">
                  A Microsoft sign-in page should be open. If it is not, open it here and enter the code below.
                </p>
                <Button type="button" variant="outline" size="sm" className="mt-3 gap-1.5" onClick={() => void handleOpenSignIn()}>
                  Open Microsoft sign-in
                  <ExternalLink className="size-3.5" />
                </Button>
              </div>

              <div className="rounded-lg border border-primary/35 bg-primary/8 p-4">
                <div className="flex items-center justify-between gap-3">
                  <p className="text-[11px] font-semibold uppercase tracking-[0.14em] text-primary">Your sign-in code</p>
                  <span className="text-[11px] text-muted-foreground">{deviceCodeCopied ? "Copied" : "Select to copy"}</span>
                </div>
                <div className="mt-2.5 flex items-stretch gap-2">
                  <code
                    className="flex min-w-0 flex-1 items-center justify-center rounded-md border border-border/80 bg-background/65 px-3 py-3 text-xl font-semibold tracking-[0.18em] text-foreground select-all"
                    aria-label={`Microsoft device code ${deviceCode.user_code}`}
                  >
                    {deviceCode.user_code}
                  </code>
                  <Button type="button" variant="outline" className="h-auto min-h-11 shrink-0 gap-1.5 px-3" onClick={() => void handleCopy()}>
                    {deviceCodeCopied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
                    {deviceCodeCopied ? "Copied" : "Copy"}
                  </Button>
                </div>
                <p className="mt-2.5 text-[11px] leading-relaxed text-muted-foreground">
                  Enter this code on the Microsoft page. The code was copied automatically when possible.
                </p>
              </div>

              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <LoaderCircle className="size-3.5 animate-spin motion-reduce:animate-none" />
                <span>Waiting for Microsoft to finish signing you in</span>
              </div>

              <p className="text-[11px] leading-relaxed text-muted-foreground">{deviceCode.message}</p>
            </div>
          )}

          {error && (
            <div className="flex items-start gap-3 rounded-lg border border-destructive/35 bg-destructive/8 px-3.5 py-3.5" role="alert">
              <CircleAlert className="mt-0.5 size-4 shrink-0 text-destructive" />
              <div>
                <p className="text-sm font-medium">{phase === "error" ? "Sign-in did not finish" : "Could not complete that action"}</p>
                <p className="mt-1 break-words text-xs leading-relaxed text-muted-foreground">{error}</p>
              </div>
            </div>
          )}
        </div>

        <div className="flex items-center justify-between gap-3 border-t border-border/80 bg-muted/15 px-5 py-3">
          <p className="text-[11px] text-muted-foreground">You can close this window and try again.</p>
          <div className="flex shrink-0 gap-2">
            <Button type="button" variant="outline" size="sm" onClick={() => handleOpenChange(false)}>
              Cancel
            </Button>
            {phase === "error" && (
              <Button type="button" size="sm" onClick={() => void startLogin()}>
                Try again
              </Button>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

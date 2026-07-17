import * as React from "react";
import { cn } from "../../lib/utils";
import { Button } from "./button";

const DialogContext = React.createContext<{
  open: boolean;
  onOpenChange: (open: boolean) => void;
} | null>(null);

function Dialog({
  open,
  onOpenChange,
  children,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  children: React.ReactNode;
}) {
  const dialogRef = React.useRef<HTMLDialogElement>(null);
  const contextValue = React.useMemo(
    () => ({ open, onOpenChange }),
    [open, onOpenChange],
  );

  React.useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  }, [open]);

  return (
    <DialogContext.Provider value={contextValue}>
      <dialog
        ref={dialogRef}
        aria-label="Application dialog"
        className="fixed inset-0 z-50 m-auto max-h-none max-w-none overflow-visible bg-transparent p-4 text-foreground backdrop:bg-black/50"
        onCancel={(event) => {
          event.preventDefault();
          onOpenChange(false);
        }}
      >
        {open ? children : null}
      </dialog>
    </DialogContext.Provider>
  );
}

function DialogContent({
  className,
  children,
  ...props
}: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn(
        "relative z-50 w-full max-w-md rounded-lg border bg-card p-6 shadow-lg",
        className,
      )}
      onClick={(e) => e.stopPropagation()}
      {...props}
    >
      {children}
    </div>
  );
}

function DialogTrigger({
  children,
  className,
  ...props
}: React.ComponentProps<typeof Button>) {
  const ctx = React.useContext(DialogContext);
  return (
    <Button
      type="button"
      className={className}
      onClick={() => ctx?.onOpenChange(true)}
      {...props}
    >
      {children}
    </Button>
  );
}

function DialogHeader({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn("flex flex-col space-y-1.5 text-center sm:text-left mb-4", className)}
      {...props}
    />
  );
}

function DialogTitle({ className, ...props }: React.HTMLAttributes<HTMLHeadingElement>) {
  return (
    <h2 className={cn("text-lg font-semibold leading-none tracking-tight", className)} {...props} />
  );
}

function DialogDescription({ className, ...props }: React.HTMLAttributes<HTMLParagraphElement>) {
  return <p className={cn("text-sm text-muted-foreground", className)} {...props} />;
}

export {
  Dialog,
  DialogContent,
  DialogTrigger,
  DialogHeader,
  DialogTitle,
  DialogDescription,
};

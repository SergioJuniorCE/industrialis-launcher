import * as React from "react";
import { cn } from "../../lib/utils";

const DialogContext = React.createContext<{
  open: boolean;
  onOpenChange: (open: boolean) => void;
} | null>(null);

function Dialog({ open, onOpenChange, children }: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  children: React.ReactNode;
}) {
  return (
    <DialogContext.Provider value={{ open, onOpenChange }}>
      {open ? (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <div
            className="fixed inset-0 bg-black/50"
            onClick={() => onOpenChange(false)}
            aria-hidden
          />
          <div
            className="relative z-50 w-full max-w-md rounded-lg border bg-card p-6 shadow-lg"
            role="dialog"
            aria-modal="true"
          >
            {children}
          </div>
        </div>
      ) : null}
    </DialogContext.Provider>
  );
}

function DialogTrigger({ children, ...props }: {
  asChild?: boolean;
  children: React.ReactNode;
  [key: string]: unknown;
}) {
  const ctx = React.useContext(DialogContext);
  return (
    <button type="button" onClick={() => ctx?.onOpenChange(true)} {...props}>
      {children}
    </button>
  );
}

function DialogHeader({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("flex flex-col space-y-1.5 text-center sm:text-left mb-4", className)} {...props} />;
}

function DialogTitle({ className, ...props }: React.HTMLAttributes<HTMLHeadingElement>) {
  return <h2 className={cn("text-lg font-semibold leading-none tracking-tight", className)} {...props} />;
}

function DialogDescription({ className, ...props }: React.HTMLAttributes<HTMLParagraphElement>) {
  return <p className={cn("text-sm text-muted-foreground", className)} {...props} />;
}

export { Dialog, DialogTrigger, DialogHeader, DialogTitle, DialogDescription };
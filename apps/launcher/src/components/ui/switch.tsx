import * as React from "react";
import { cn } from "../../lib/utils";

export interface SwitchProps extends Omit<React.ButtonHTMLAttributes<HTMLButtonElement>, "onChange"> {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  label?: React.ReactNode;
}

const Switch = React.forwardRef<HTMLButtonElement, SwitchProps>(
  ({ className, checked, onCheckedChange, label, disabled, id, ...props }, ref) => {
    const autoId = React.useId();
    const switchId = id ?? autoId;

    return (
      <label
        htmlFor={switchId}
        className={cn(
          "inline-flex items-center gap-2 text-xs cursor-pointer select-none",
          disabled && "opacity-50 cursor-not-allowed",
          className,
        )}
      >
        <button
          ref={ref}
          id={switchId}
          type="button"
          role="switch"
          aria-checked={checked}
          disabled={disabled}
          onClick={() => onCheckedChange(!checked)}
          className={cn(
            "relative inline-flex h-5 w-9 shrink-0 items-center rounded-full border border-input transition-colors",
            checked ? "bg-primary" : "bg-muted",
          )}
          {...props}
        >
          <span
            className={cn(
              "pointer-events-none block size-3.5 rounded-full bg-background shadow transition-transform",
              checked ? "translate-x-4" : "translate-x-0.5",
            )}
          />
        </button>
        {label ? <span className="min-w-0">{label}</span> : null}
      </label>
    );
  },
);
Switch.displayName = "Switch";

export { Switch };
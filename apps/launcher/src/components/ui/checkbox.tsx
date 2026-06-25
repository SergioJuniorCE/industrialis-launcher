import * as React from "react";
import { cn } from "../../lib/utils";

export interface CheckboxProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, "type"> {
  label?: React.ReactNode;
}

const Checkbox = React.forwardRef<HTMLInputElement, CheckboxProps>(
  ({ className, label, id, ...props }, ref) => {
    const autoId = React.useId();
    const inputId = id ?? autoId;
    return (
      <label
        htmlFor={inputId}
        className={cn(
          "flex items-start gap-2 text-sm leading-snug cursor-pointer",
          props.disabled && "opacity-50 cursor-not-allowed",
          className
        )}
      >
        <input
          ref={ref}
          id={inputId}
          type="checkbox"
          className="mt-0.5 size-4 shrink-0 rounded border border-input accent-accent"
          {...props}
        />
        {label ? <span className="min-w-0">{label}</span> : null}
      </label>
    );
  }
);
Checkbox.displayName = "Checkbox";

export { Checkbox };
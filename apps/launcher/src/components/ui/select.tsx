import * as React from "react";
import { ChevronDown } from "lucide-react";
import { cn } from "../../lib/utils";

type SelectOption = {
  value: string;
  label: string;
  disabled?: boolean;
};

function optionLabel(children: React.ReactNode): string {
  if (typeof children === "string" || typeof children === "number") {
    return String(children);
  }
  return React.Children.toArray(children).join("");
}

function parseOptions(children: React.ReactNode): SelectOption[] {
  const options: SelectOption[] = [];
  React.Children.forEach(children, (child) => {
    if (!React.isValidElement<{ value?: string; disabled?: boolean; children?: React.ReactNode }>(child)) {
      return;
    }
    if (child.type !== "option") return;
    options.push({
      value: String(child.props.value ?? ""),
      label: optionLabel(child.props.children),
      disabled: child.props.disabled,
    });
  });
  return options;
}

function emitChange(
  onChange: React.SelectHTMLAttributes<HTMLSelectElement>["onChange"],
  value: string,
) {
  if (!onChange) return;
  onChange({
    target: { value },
    currentTarget: { value },
  } as React.ChangeEvent<HTMLSelectElement>);
}

const Select = React.forwardRef<HTMLButtonElement, React.SelectHTMLAttributes<HTMLSelectElement>>(
  ({ className, children, value, defaultValue, onChange, disabled, name, id }, ref) => {
    const [open, setOpen] = React.useState(false);
    const [uncontrolledValue, setUncontrolledValue] = React.useState(String(defaultValue ?? ""));
    const containerRef = React.useRef<HTMLDivElement>(null);
    const options = React.useMemo(() => parseOptions(children), [children]);
    const currentValue = value !== undefined ? String(value) : uncontrolledValue;
    const selected = options.find((option) => option.value === currentValue);

    React.useEffect(() => {
      if (!open) return;
      const handleClick = (event: MouseEvent) => {
        if (containerRef.current && !containerRef.current.contains(event.target as Node)) {
          setOpen(false);
        }
      };
      document.addEventListener("mousedown", handleClick);
      return () => document.removeEventListener("mousedown", handleClick);
    }, [open]);

    const choose = (nextValue: string) => {
      if (value === undefined) {
        setUncontrolledValue(nextValue);
      }
      emitChange(onChange, nextValue);
      setOpen(false);
    };

    return (
      <div ref={containerRef} className={cn("relative", className)}>
        {name ? <input type="hidden" name={name} value={currentValue} /> : null}
        <button
          ref={ref}
          id={id}
          type="button"
          disabled={disabled}
          aria-haspopup="listbox"
          aria-expanded={open}
          onClick={() => setOpen((open) => !open)}
          className={cn(
            "flex h-9 w-full items-center justify-between gap-2 rounded-md border border-input bg-card px-3 py-1 text-sm text-foreground shadow-sm transition-colors",
            "hover:border-primary/40 hover:bg-accent/40",
            "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
            "disabled:cursor-not-allowed disabled:opacity-50",
            open && "border-primary/50 ring-1 ring-ring/60",
          )}
        >
          <span className="truncate">{selected?.label ?? currentValue}</span>
          <ChevronDown
            className={cn(
              "size-4 shrink-0 text-muted-foreground transition-transform",
              open && "rotate-180",
            )}
          />
        </button>

        {open && (
          <div
            role="listbox"
            className="absolute left-0 right-0 top-full z-[60] mt-1 overflow-hidden rounded-md border border-border bg-popover text-popover-foreground shadow-lg"
          >
            {options.map((option) => {
              const isSelected = option.value === currentValue;
              return (
                <button
                  key={option.value}
                  type="button"
                  role="option"
                  aria-selected={isSelected}
                  disabled={option.disabled}
                  onClick={() => choose(option.value)}
                  className={cn(
                    "flex w-full px-3 py-2 text-left text-sm transition-colors",
                    isSelected
                      ? "bg-primary/18 text-foreground"
                      : "text-foreground hover:bg-accent hover:text-accent-foreground",
                    option.disabled && "pointer-events-none opacity-50",
                  )}
                >
                  {option.label}
                </button>
              );
            })}
          </div>
        )}
      </div>
    );
  },
);
Select.displayName = "Select";

export { Select };
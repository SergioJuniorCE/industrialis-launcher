import * as React from "react";
import { cn } from "../../lib/utils";
import { Button } from "./button";

const TabsContext = React.createContext<{
  value: string;
  onValueChange: (value: string) => void;
  baseId: string;
  orientation: "horizontal" | "vertical";
} | null>(null);

interface TabsProps extends React.HTMLAttributes<HTMLDivElement> {
  value: string;
  onValueChange: (value: string) => void;
  orientation?: "horizontal" | "vertical";
}

function tabId(baseId: string, kind: "trigger" | "panel", value: string) {
  return `${baseId}-${kind}-${encodeURIComponent(value)}`;
}

function Tabs({ value, onValueChange, orientation = "horizontal", className, children, ...props }: TabsProps) {
  const generatedId = React.useId();
  const baseId = `tabs-${generatedId.replaceAll(":", "")}`;
  const contextValue = React.useMemo(() => ({ value, onValueChange, baseId, orientation }), [value, onValueChange, baseId, orientation]);
  return (
    <TabsContext.Provider value={contextValue}>
      <div className={cn("w-full", className)} {...props}>
        {children}
      </div>
    </TabsContext.Provider>
  );
}

function TabsList({ className, children, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  const ctx = React.useContext(TabsContext);

  return (
    <div
      role="tablist"
      aria-orientation={ctx?.orientation === "vertical" ? "vertical" : undefined}
      className={cn("inline-flex h-7 items-center justify-center rounded-md bg-muted p-0.5 text-muted-foreground", className)}
      {...props}
    >
      {children}
    </div>
  );
}

interface TabsTriggerProps extends Omit<React.ComponentProps<typeof Button>, "value"> {
  value: string;
}

function TabsTrigger({ value, className, children, disabled, onClick, onKeyDown, ...props }: TabsTriggerProps) {
  const ctx = React.useContext(TabsContext);
  const isActive = ctx?.value === value;
  const orientation = ctx?.orientation ?? "horizontal";

  const handleClick = (event: React.MouseEvent<HTMLButtonElement>) => {
    onClick?.(event);
    if (!event.defaultPrevented) ctx?.onValueChange(value);
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLButtonElement>) => {
    onKeyDown?.(event);
    const navigationKeys = orientation === "vertical" ? ["ArrowUp", "ArrowDown", "Home", "End"] : ["ArrowLeft", "ArrowRight", "Home", "End"];
    if (event.defaultPrevented || !navigationKeys.includes(event.key)) return;

    const tabList = event.currentTarget.closest<HTMLElement>('[role="tablist"]');
    const tabs = Array.from(tabList?.querySelectorAll<HTMLButtonElement>('[role="tab"]:not(:disabled)') ?? []);
    const currentIndex = tabs.indexOf(event.currentTarget);
    if (currentIndex === -1 || tabs.length === 0) return;

    event.preventDefault();
    const nextIndex =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? tabs.length - 1
          : (currentIndex + (event.key === "ArrowDown" || event.key === "ArrowRight" ? 1 : -1) + tabs.length) % tabs.length;
    tabs[nextIndex]?.focus();
    tabs[nextIndex]?.click();
  };

  return (
    <Button
      {...props}
      type="button"
      role="tab"
      id={ctx ? tabId(ctx.baseId, "trigger", value) : undefined}
      aria-controls={ctx ? tabId(ctx.baseId, "panel", value) : undefined}
      aria-selected={isActive}
      tabIndex={isActive ? 0 : -1}
      variant="ghost"
      size="sm"
      disabled={disabled}
      onClick={handleClick}
      onKeyDown={handleKeyDown}
      className={cn(
        "h-7 rounded-sm px-2.5 py-0.5 text-xs font-medium shadow-none",
        isActive ? "bg-background text-foreground shadow" : "hover:text-foreground",
        className,
      )}
    >
      {children}
    </Button>
  );
}

interface TabsContentProps extends React.HTMLAttributes<HTMLDivElement> {
  value: string;
}

function TabsContent({ value, className, children, ...props }: TabsContentProps) {
  const ctx = React.useContext(TabsContext);
  if (ctx?.value !== value) return null;
  return (
    <div
      role="tabpanel"
      id={ctx ? tabId(ctx.baseId, "panel", value) : undefined}
      aria-labelledby={ctx ? tabId(ctx.baseId, "trigger", value) : undefined}
      className={cn("mt-2", className)}
      {...props}
    >
      {children}
    </div>
  );
}

export { Tabs, TabsList, TabsTrigger, TabsContent };

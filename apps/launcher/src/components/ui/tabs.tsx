import * as React from "react";
import { cn } from "../../lib/utils";
import { Button } from "./button";

const TabsContext = React.createContext<{
  value: string;
  onValueChange: (value: string) => void;
} | null>(null);

function Tabs({ value, onValueChange, className, children, ...props }: {
  value: string;
  onValueChange: (value: string) => void;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <TabsContext.Provider value={{ value, onValueChange }}>
      <div className={cn("w-full", className)} {...props}>
        {children}
      </div>
    </TabsContext.Provider>
  );
}

function TabsList({ className, children, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn(
        "inline-flex h-7 items-center justify-center rounded-md bg-muted p-0.5 text-muted-foreground",
        className
      )}
      {...props}
    >
      {children}
    </div>
  );
}

function TabsTrigger({ value, className, children, disabled, ...props }: {
  value: string;
  className?: string;
  children: React.ReactNode;
  disabled?: boolean;
}) {
  const ctx = React.useContext(TabsContext);
  const isActive = ctx?.value === value;
  return (
    <Button
      type="button"
      variant="ghost"
      size="sm"
      disabled={disabled}
      onClick={() => ctx?.onValueChange(value)}
      className={cn(
        "h-7 rounded-sm px-2.5 py-0.5 text-xs font-medium shadow-none",
        isActive ? "bg-background text-foreground shadow" : "hover:text-foreground",
        className,
      )}
      {...props}
    >
      {children}
    </Button>
  );
}

function TabsContent({ value, className, children, ...props }: {
  value: string;
  className?: string;
  children: React.ReactNode;
}) {
  const ctx = React.useContext(TabsContext);
  if (ctx?.value !== value) return null;
  return (
    <div className={cn("mt-2", className)} {...props}>
      {children}
    </div>
  );
}

export { Tabs, TabsList, TabsTrigger, TabsContent };

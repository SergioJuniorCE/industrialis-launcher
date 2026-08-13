import { Switch } from "./ui/switch";

export type UiComparisonMode = "before" | "after";

export function UiComparisonSwitch({ mode, onModeChange }: { mode: UiComparisonMode; onModeChange: (mode: UiComparisonMode) => void }) {
  const flattened = mode === "after";

  return (
    <div className="ui-comparison-toggle" role="group" aria-label="Interface comparison">
      <span className="ui-comparison-label" data-active={!flattened}>
        Before
      </span>
      <Switch
        checked={flattened}
        onCheckedChange={(checked) => onModeChange(checked ? "after" : "before")}
        aria-label="Use flattened interface"
        title="Switch between the original and flattened interface"
      />
      <span className="ui-comparison-label" data-active={flattened}>
        After
      </span>
    </div>
  );
}

import { useMemo } from "react";
import { ScrollArea } from "./ui/scroll-area";
import { Switch } from "./ui/switch";
import { Input } from "./ui/input";
import { Label } from "./ui/label";
import {
  type ForgeConfigDocument,
  type ForgeConfigField,
  updateForgeField,
} from "../lib/forge-cfg";

interface ForgeConfigEasyEditorProps {
  document: ForgeConfigDocument;
  onChange: (next: string) => void;
  serialize: (doc: ForgeConfigDocument) => string;
}

function FieldControl({
  sectionName,
  field,
  onFieldChange,
}: {
  sectionName: string;
  field: ForgeConfigField;
  onFieldChange: (sectionName: string, key: string, value: string | boolean | number) => void;
}) {
  const id = `${sectionName}-${field.key}`.replace(/[^a-zA-Z0-9_-]/g, "_");

  if (field.type === "bool") {
    return (
      <div className="flex items-start justify-between gap-3 py-1.5">
        <div className="min-w-0 flex-1">
          <Label htmlFor={id} className="text-xs font-medium">
            {field.key}
          </Label>
          {field.description && (
            <p className="text-[10px] text-muted-foreground leading-snug mt-0.5">{field.description}</p>
          )}
        </div>
        <Switch
          id={id}
          checked={Boolean(field.value)}
          onCheckedChange={(checked) => onFieldChange(sectionName, field.key, checked)}
        />
      </div>
    );
  }

  if (field.type === "int" || field.type === "double") {
    return (
      <div className="py-1.5 space-y-1">
        <Label htmlFor={id} className="text-xs font-medium">
          {field.key}
        </Label>
        {field.description && (
          <p className="text-[10px] text-muted-foreground leading-snug">{field.description}</p>
        )}
        <Input
          id={id}
          type="number"
          className="h-7 text-xs"
          value={String(field.value)}
          onChange={(e) => {
            const raw = e.target.value;
            const next =
              field.type === "int"
                ? Number.parseInt(raw, 10) || 0
                : Number.parseFloat(raw) || 0;
            onFieldChange(sectionName, field.key, next);
          }}
        />
      </div>
    );
  }

  if (field.multiline) {
    return (
      <div className="py-1.5 space-y-1">
        <Label htmlFor={id} className="text-xs font-medium">
          {field.key}
        </Label>
        {field.description && (
          <p className="text-[10px] text-muted-foreground leading-snug">{field.description}</p>
        )}
        <textarea
          id={id}
          className="w-full min-h-[72px] rounded-md border border-input bg-transparent px-2 py-1.5 text-xs font-mono"
          value={String(field.value)}
          onChange={(e) => onFieldChange(sectionName, field.key, e.target.value)}
        />
      </div>
    );
  }

  return (
    <div className="py-1.5 space-y-1">
      <Label htmlFor={id} className="text-xs font-medium">
        {field.key}
      </Label>
      {field.description && (
        <p className="text-[10px] text-muted-foreground leading-snug">{field.description}</p>
      )}
      <Input
        id={id}
        className="h-7 text-xs font-mono"
        value={String(field.value)}
        onChange={(e) => onFieldChange(sectionName, field.key, e.target.value)}
      />
    </div>
  );
}

export function ForgeConfigEasyEditor({ document, onChange, serialize }: ForgeConfigEasyEditorProps) {
  const editableFields = useMemo(
    () =>
      document.sections.flatMap((section) =>
        section.fields
          .filter((field) => field.type !== "unknown")
          .map((field) => ({ section, field })),
      ),
    [document],
  );

  const handleFieldChange = (
    sectionName: string,
    key: string,
    value: string | boolean | number,
  ) => {
    const nextDoc = updateForgeField(document, sectionName, key, value);
    onChange(serialize(nextDoc));
  };

  return (
    <ScrollArea className="flex-1 min-h-0">
      <div className="p-3 space-y-4">
        {document.sections.map((section) => {
          const fields = section.fields.filter((field) => field.type !== "unknown");
          if (fields.length === 0) return null;
          return (
            <section key={section.name} className="rounded-md border border-border bg-background/40">
              <h3 className="px-3 py-2 text-xs font-semibold border-b border-border bg-muted/30">
                {section.name}
              </h3>
              <div className="px-3 py-1 divide-y divide-border/60">
                {fields.map((field) => (
                  <FieldControl
                    key={`${section.name}-${field.key}`}
                    sectionName={section.name}
                    field={field}
                    onFieldChange={handleFieldChange}
                  />
                ))}
              </div>
            </section>
          );
        })}
        {editableFields.length === 0 && (
          <p className="text-xs text-muted-foreground">
            No editable fields found. Switch to Advanced mode to edit this file directly.
          </p>
        )}
      </div>
    </ScrollArea>
  );
}
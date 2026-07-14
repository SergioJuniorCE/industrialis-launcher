export type ForgeValueType = "bool" | "int" | "string" | "double" | "unknown";

export interface ForgeConfigField {
  key: string;
  type: ForgeValueType;
  value: string | boolean | number;
  description?: string;
  multiline?: boolean;
}

export interface ForgeConfigSection {
  name: string;
  fields: ForgeConfigField[];
}

export interface ForgeConfigDocument {
  headerComments: string[];
  sections: ForgeConfigSection[];
}

const PROPERTY_LINE =
  /^([BISD]):(?:"([^"]+)"|([^=<]+?))\s*=\s*(.*)$/;
const MULTILINE_OPEN =
  /^([BISD]):(?:"([^"]+)"|([^<]+?))\s*<\s*$/;

function parseValue(type: ForgeValueType, raw: string): string | boolean | number {
  const trimmed = raw.trim();
  if (type === "bool") return trimmed.toLowerCase() === "true";
  if (type === "int") {
    const n = Number.parseInt(trimmed, 10);
    return Number.isNaN(n) ? 0 : n;
  }
  if (type === "double") {
    const n = Number.parseFloat(trimmed);
    return Number.isNaN(n) ? 0 : n;
  }
  return trimmed;
}

function formatValue(type: ForgeValueType, value: string | boolean | number): string {
  if (type === "bool") return value ? "true" : "false";
  return String(value);
}

export function isForgeConfigFile(path: string): boolean {
  const lower = path.toLowerCase();
  return lower.endsWith(".cfg") || lower.endsWith(".properties");
}

export function parseForgeConfig(content: string): ForgeConfigDocument | null {
  const lines = content.replace(/\r\n/g, "\n").split("\n");
  const doc: ForgeConfigDocument = { headerComments: [], sections: [] };

  let section: ForgeConfigSection | null = null;
  let pendingComment: string | undefined;
  let i = 0;

  const openSection = (name: string) => {
    section = { name, fields: [] };
    doc.sections.push(section);
  };

  while (i < lines.length) {
    const line = lines[i];
    const trimmed = line.trim();

    if (!trimmed) {
      i += 1;
      continue;
    }

    if (trimmed.startsWith("#")) {
      const comment = trimmed.slice(1).trim();
      if (!section) {
        doc.headerComments.push(comment);
      } else {
        pendingComment = comment;
      }
      i += 1;
      continue;
    }

    const sectionMatch = trimmed.match(/^("([^"]+)"|([^"{]+))\s*\{\s*$/);
    if (sectionMatch) {
      openSection(sectionMatch[2] ?? sectionMatch[3]!.trim());
      pendingComment = undefined;
      i += 1;
      continue;
    }

    if (trimmed === "}") {
      section = null;
      pendingComment = undefined;
      i += 1;
      continue;
    }

    if (!section) {
      return null;
    }
    const activeSection: ForgeConfigSection = section;

    const typeFromLetter = (letter: string): ForgeValueType =>
      letter === "B"
        ? "bool"
        : letter === "I"
          ? "int"
          : letter === "S"
            ? "string"
            : letter === "D"
              ? "double"
              : "unknown";

    const multilineMatch = trimmed.match(MULTILINE_OPEN);
    if (multilineMatch) {
      const typeLetter = multilineMatch[1]!;
      const key = (multilineMatch[2] ?? multilineMatch[3] ?? "").trim();
      const parts: string[] = [];
      i += 1;
      while (i < lines.length) {
        const inner = lines[i]!.trim();
        if (inner === ">") break;
        if (inner.length > 0) parts.push(inner);
        i += 1;
      }
      activeSection.fields.push({
        key,
        type: typeFromLetter(typeLetter),
        value: parts.join("\n"),
        description: pendingComment,
        multiline: true,
      });
      pendingComment = undefined;
      i += 1;
      continue;
    }

    const propMatch = trimmed.match(PROPERTY_LINE);
    if (!propMatch) {
      return null;
    }

    const typeLetter = propMatch[1]!;
    const key = (propMatch[2] ?? propMatch[3] ?? "").trim();
    const rawValue = propMatch[4] ?? "";

    activeSection.fields.push({
      key,
      type: typeFromLetter(typeLetter),
      value: parseValue(typeFromLetter(typeLetter), rawValue),
      description: pendingComment,
      multiline: false,
    });
    pendingComment = undefined;
    i += 1;
  }

  return doc.sections.length > 0 || doc.headerComments.length > 0 ? doc : null;
}

export function serializeForgeConfig(doc: ForgeConfigDocument): string {
  const out: string[] = ["# Configuration file", ""];

  for (const comment of doc.headerComments) {
    out.push(`# ${comment}`);
  }
  if (doc.headerComments.length > 0) out.push("");

  for (const section of doc.sections) {
    const needsQuotes =
      section.name.includes("|") ||
      section.name.includes(" ") ||
      section.name.includes(":");
    const sectionName = needsQuotes ? `"${section.name}"` : section.name;
    out.push(`${sectionName} {`);

    for (const field of section.fields) {
      if (field.description) {
        out.push(`    # ${field.description}`);
      }
      const prefix =
        field.type === "bool"
          ? "B"
          : field.type === "int"
            ? "I"
            : field.type === "double"
              ? "D"
              : "S";
      const keyLabel =
        field.key.includes(" ") || field.key.includes("|")
          ? `"${field.key}"`
          : field.key;

      if (field.multiline && field.type === "string") {
        const lines = String(field.value).split("\n").filter((l) => l.length > 0);
        out.push(`    ${prefix}:${keyLabel} <`);
        for (const line of lines) {
          out.push(`        ${line}`);
        }
        out.push("     >");
      } else {
        out.push(`    ${prefix}:${keyLabel}=${formatValue(field.type, field.value)}`);
      }
    }

    out.push("}");
    out.push("");
  }

  return `${out.join("\n").trimEnd()}\n`;
}

export function updateForgeField(
  doc: ForgeConfigDocument,
  sectionName: string,
  key: string,
  value: string | boolean | number,
): ForgeConfigDocument {
  return {
    ...doc,
    sections: doc.sections.map((section) =>
      section.name === sectionName
        ? {
            ...section,
            fields: section.fields.map((field) =>
              field.key === key ? { ...field, value } : field,
            ),
          }
        : section,
    ),
  };
}
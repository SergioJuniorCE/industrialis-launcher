export type MinecraftEditorMode = "easy" | "advanced";

const MODE_KEY = "industrialis-minecraft-editor-mode";

export function readMinecraftEditorMode(): MinecraftEditorMode {
  try {
    const raw = localStorage.getItem(MODE_KEY);
    return raw === "easy" ? "easy" : "advanced";
  } catch {
    return "advanced";
  }
}

export function writeMinecraftEditorMode(mode: MinecraftEditorMode): void {
  try {
    localStorage.setItem(MODE_KEY, mode);
  } catch {
    // ignore quota / private mode
  }
}
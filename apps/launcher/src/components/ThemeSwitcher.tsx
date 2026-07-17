import { Moon, Sun } from "lucide-react";
import { Button } from "./ui/button";
import { useLauncherSettings } from "../context/launcher-settings-context";

export function ThemeSwitcher() {
  const { settings, setThemeMode } = useLauncherSettings();
  const isDark = settings.theme_mode === "dark";

  return (
    <Button
      variant="ghost"
      size="icon"
      onClick={() => setThemeMode(isDark ? "light" : "dark")}
      aria-label={isDark ? "Switch to light mode" : "Switch to dark mode"}
      aria-pressed={isDark}
      title={isDark ? "Light mode" : "Dark mode"}
    >
      {isDark ? <Sun className="size-4" /> : <Moon className="size-4" />}
    </Button>
  );
}

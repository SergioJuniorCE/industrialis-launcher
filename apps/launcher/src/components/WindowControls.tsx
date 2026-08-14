import { useEffect, useState } from "react";
import { Copy, Minus, Square, X } from "lucide-react";
import { closeWindow, desktopPlatform, isDesktop, isWindowMaximized, listen, minimizeWindow, toggleMaximizeWindow } from "../lib/desktop";

interface WindowMaximizedEvent {
  maximized: boolean;
}

export function WindowControls() {
  const [maximized, setMaximized] = useState(false);
  const desktop = isDesktop();

  useEffect(() => {
    if (!desktop) return;

    let active = true;
    void isWindowMaximized()
      .then((value) => {
        if (active) setMaximized(value);
      })
      .catch(() => undefined);

    const unlisten = listen<WindowMaximizedEvent>("window-maximized", (event) => {
      setMaximized(event.payload.maximized);
    });

    return () => {
      active = false;
      void unlisten.then((unsubscribe) => unsubscribe());
    };
  }, [desktop]);

  const handleToggleMaximize = async () => {
    if (!desktop) return;
    try {
      setMaximized(await toggleMaximizeWindow());
    } catch {
      // The native window may close while the command is in flight.
    }
  };

  if (!desktop || desktopPlatform() === "darwin") return null;

  return (
    <div className="window-controls" aria-label="Window controls">
      <button
        type="button"
        className="window-control"
        aria-label="Minimize window"
        title="Minimize"
        onClick={() => {
          if (desktop) void minimizeWindow();
        }}
      >
        <Minus aria-hidden="true" />
      </button>
      <button
        type="button"
        className="window-control"
        aria-label={maximized ? "Restore window" : "Maximize window"}
        title={maximized ? "Restore" : "Maximize"}
        onClick={() => void handleToggleMaximize()}
      >
        {maximized ? <Copy aria-hidden="true" /> : <Square aria-hidden="true" />}
      </button>
      <button
        type="button"
        className="window-control window-control-close"
        aria-label="Close window"
        title="Close"
        onClick={() => {
          if (desktop) void closeWindow();
        }}
      >
        <X aria-hidden="true" />
      </button>
    </div>
  );
}

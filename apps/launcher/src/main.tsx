if (import.meta.env.DEV) {
  void import("react-grab");
}

import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { LauncherSettingsProvider } from "./context/LauncherSettingsContext";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <LauncherSettingsProvider>
      <App />
    </LauncherSettingsProvider>
  </React.StrictMode>,
);

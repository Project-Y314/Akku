import React from "react";
import ReactDOM from "react-dom/client";

export function AppPlaceholder() {
  return (
    <div className="p-3 rounded-xl bg-zinc-950/95 border border-zinc-800 animate-tray-popover-in">
      <div className="text-xs font-mono text-zinc-400">
        PeripheralDeck Scaffold Ready (UI populated in Deliverable 6)
      </div>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <AppPlaceholder />
  </React.StrictMode>
);

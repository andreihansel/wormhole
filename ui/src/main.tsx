import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { TooltipProvider } from "@radix-ui/react-tooltip";
import "@fontsource/ibm-plex-sans/400.css";
import "@fontsource/ibm-plex-sans/500.css";
import "@fontsource/ibm-plex-sans/600.css";
import "@fontsource/ibm-plex-mono/400.css";
import "./styles.css";

async function main() {
  // End-to-end tests run the UI in a browser against an in-memory backend.
  if (import.meta.env.VITE_E2E) await import("./e2e/fakeBackend");
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <TooltipProvider delayDuration={350}>
        <App />
      </TooltipProvider>
    </StrictMode>,
  );
}

void main();

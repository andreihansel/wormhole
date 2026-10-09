import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "e2e",
  outputDir: "e2e/.out",
  use: { baseURL: "http://localhost:1421", viewport: { width: 1280, height: 800 }, colorScheme: "dark" },
  webServer: {
    command: "VITE_E2E=1 npx vite --port 1421 --strictPort",
    url: "http://localhost:1421",
    reuseExistingServer: false,
    timeout: 60_000,
  },
});

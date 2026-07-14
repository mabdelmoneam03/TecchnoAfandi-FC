import { defineConfig } from "vite";
import { resolve } from "path";
import { fileURLToPath } from "url";

// ESM-safe __dirname
const __dirname = fileURLToPath(new URL(".", import.meta.url));

export default defineConfig({
  root: "src",
  publicDir: "../public",
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    rollupOptions: {
      input: {
        home: resolve(__dirname, "src/home_page_v2.html"),
        version: resolve(__dirname, "src/version_page_v2.html"),
      },
    },
  },
  server: {
    port: 1420,
    strictPort: true,
  },
  clearScreen: false,
});

import { defineConfig } from "vite";

export default defineConfig({
  base: "/adminpanel/",
  build: { outDir: "dist", emptyOutDir: true },
  server: { port: 1420, strictPort: true },
});

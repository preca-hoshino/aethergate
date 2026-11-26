import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { brotliCompressSync, constants as zlibConstants, gzipSync } from "node:zlib";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const nodeModulesDir = fileURLToPath(new URL("./node_modules", import.meta.url));
const pkgJsonPath = fileURLToPath(new URL("./package.json", import.meta.url));
const distDir = fileURLToPath(new URL("./dist", import.meta.url));

function collectThirdPartyNotices(): string {
  const pkg = JSON.parse(readFileSync(pkgJsonPath, "utf8")) as {
    dependencies?: Record<string, string>;
  };
  const seeds = Object.keys(pkg.dependencies ?? {});
  const visited = new Set<string>();
  const queue = [...seeds];
  const sections: string[] = [];

  while (queue.length > 0) {
    const name = queue.shift();
    if (!name || visited.has(name)) continue;
    visited.add(name);

    const pkgDir = join(nodeModulesDir, name);
    if (!existsSync(pkgDir)) continue;

    const subPkgPath = join(pkgDir, "package.json");
    if (existsSync(subPkgPath)) {
      try {
        const subPkg = JSON.parse(readFileSync(subPkgPath, "utf8")) as {
          dependencies?: Record<string, string>;
        };
        for (const dep of Object.keys(subPkg.dependencies ?? {})) {
          if (!visited.has(dep)) queue.push(dep);
        }
      } catch {
        // Ignore malformed package manifests during license scan.
      }
    }

    const entries = readdirSync(pkgDir, { withFileTypes: true });
    for (const entry of entries) {
      if (!entry.isFile()) continue;
      if (/^(licen[cs]e|thirdparty|notice)/i.test(entry.name)) {
        const content = readFileSync(join(pkgDir, entry.name), "utf8");
        sections.push(`${name}/${entry.name}\n\n${content}`);
      }
    }
  }

  return sections.join("\n\n");
}

function precompressDist(dir: string): void {
  if (!existsSync(dir)) return;
  const entries = readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      precompressDist(full);
      continue;
    }
    if (!/\.(html|js|css|svg|json|txt)$/i.test(entry.name)) continue;
    const raw = readFileSync(full);
    if (raw.length < 256) continue;
    const br = brotliCompressSync(raw, {
      params: { [zlibConstants.BROTLI_PARAM_QUALITY]: 11 },
    });
    const gz = gzipSync(raw, { level: 9 });
    mkdirSync(dirname(full), { recursive: true });
    writeFileSync(`${full}.br`, br);
    writeFileSync(`${full}.gz`, gz);
  }
}

export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
    {
      name: "embedded-notices-and-precompress",
      apply: "build",
      generateBundle() {
        this.emitFile({
          type: "asset",
          fileName: "third-party-notices.txt",
          source: collectThirdPartyNotices(),
        });
      },
      closeBundle() {
        precompressDist(distDir);
      },
    },
  ],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  server: {
    port: 5173,
    proxy: {
      "/api": {
        target: "http://127.0.0.1:8080",
        ws: true,
      },
      "/sub": "http://127.0.0.1:8080",
      "/healthz": "http://127.0.0.1:8080",
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes("node_modules/recharts") || id.includes("node_modules/d3-")) {
            return "charts";
          }
          if (id.includes("node_modules/@tanstack/")) return "tanstack";
          if (id.includes("node_modules/@radix-ui/")) return "radix";
          return undefined;
        },
      },
    },
  },
});

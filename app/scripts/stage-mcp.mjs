import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const root = resolve(appDir, "..");
const built = resolve(root, "target", "release", "devkit-mcp.exe");
const destDir = resolve(appDir, "src-tauri", "resources");
const dest = resolve(destDir, "devkit-mcp.exe");

const result = spawnSync("cargo build -p devkit-mcp --release", {
  cwd: root,
  stdio: "inherit",
  shell: true,
});
if (result.status !== 0 || !existsSync(built)) {
  console.error("No se pudo generar devkit-mcp.exe durante el build del instalador.");
  process.exit(result.status || 1);
}

mkdirSync(destDir, { recursive: true });
copyFileSync(built, dest);
console.log(`devkit-mcp.exe listo para el instalador: ${dest}`);

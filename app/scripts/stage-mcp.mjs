import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const root = resolve(appDir, "..");
const built = resolve(root, "target", "release", "devkit-mcp.exe");
const destDir = resolve(appDir, "src-tauri", "resources");
const dest = resolve(destDir, "devkit-mcp.exe");
const cliBuilt = resolve(root, "target", "release", "devkit.exe");
const cliDest = resolve(destDir, "devkit.exe");

function buildRelease(args, label) {
  const result = spawnSync(`cargo ${args}`, {
    cwd: root,
    stdio: "inherit",
    shell: true,
  });
  if (result.status !== 0) {
    console.error(`No se pudo generar ${label} durante el build del instalador.`);
    process.exit(result.status || 1);
  }
}

buildRelease("build -p devkit-mcp --release", "devkit-mcp.exe");
buildRelease("build -p devkit-cli --release", "devkit.exe");

if (!existsSync(built) || !existsSync(cliBuilt)) {
  console.error("Faltan devkit-mcp.exe o devkit.exe en target/release.");
  process.exit(1);
}

mkdirSync(destDir, { recursive: true });
copyFileSync(built, dest);
copyFileSync(cliBuilt, cliDest);
console.log(`devkit-mcp.exe listo para el instalador: ${dest}`);
console.log(`devkit.exe listo para el instalador: ${cliDest}`);

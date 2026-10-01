import { copyFileSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// tauri dev comprueba que el recurso del instalador exista antes de compilar.
// No compila devkit-mcp: si hay un release previo lo copia; si no, deja un marcador.
const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const root = resolve(appDir, "..");
const destDir = resolve(appDir, "src-tauri", "resources");

function ensureExe(built, dest, label) {
  if (existsSync(built)) {
    copyFileSync(built, dest);
    console.log(`recurso ${label}: copia de target/release`);
  } else if (!existsSync(dest)) {
    writeFileSync(dest, "");
    console.log(`recurso ${label}: marcador vacío (el instalador lo sustituye al empaquetar)`);
  }
}

mkdirSync(destDir, { recursive: true });
ensureExe(
  resolve(root, "target", "release", "devkit-mcp.exe"),
  resolve(destDir, "devkit-mcp.exe"),
  "devkit-mcp.exe"
);
ensureExe(
  resolve(root, "target", "release", "devkit.exe"),
  resolve(destDir, "devkit.exe"),
  "devkit.exe"
);

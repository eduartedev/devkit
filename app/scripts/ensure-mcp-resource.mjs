import { copyFileSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// tauri dev comprueba que el recurso del instalador exista antes de compilar.
// No compila devkit-mcp: si hay un release previo lo copia; si no, deja un marcador.
const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const root = resolve(appDir, "..");
const built = resolve(root, "target", "release", "devkit-mcp.exe");
const destDir = resolve(appDir, "src-tauri", "resources");
const dest = resolve(destDir, "devkit-mcp.exe");

mkdirSync(destDir, { recursive: true });
if (existsSync(built)) {
  copyFileSync(built, dest);
  console.log("recurso MCP: copia de target/release/devkit-mcp.exe");
} else if (!existsSync(dest)) {
  writeFileSync(dest, "");
  console.log("recurso MCP: marcador vacío (el instalador lo sustituye al empaquetar)");
}

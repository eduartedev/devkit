import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);

const tauriArgs = [...args];
if (args[0] === "build") {
  tauriArgs.push("--config", "src-tauri/tauri.offline.json");
}

const run = spawnSync("npx", ["tauri", ...tauriArgs], {
  cwd: appDir,
  stdio: "inherit",
  shell: true,
});
const code = run.status ?? 1;
if (code !== 0) {
  process.exit(code);
}

if (args[0] === "build") {
  const copy = spawnSync(process.execPath, ["scripts/copy-to-kit-dist.mjs"], {
    cwd: appDir,
    stdio: "inherit",
  });
  process.exit(copy.status ?? 1);
}

import { copyFileSync, existsSync, mkdirSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const kitRoot = resolve(appDir, "..");
const distDir = join(kitRoot, "dist");

function cargoMetadataTarget() {
  const meta = spawnSync(
    "cargo",
    [
      "metadata",
      "--format-version",
      "1",
      "--no-deps",
      "--manifest-path",
      join(appDir, "src-tauri", "Cargo.toml"),
    ],
    { cwd: kitRoot, encoding: "utf8" }
  );
  if (meta.status === 0 && meta.stdout) {
    try {
      const json = JSON.parse(meta.stdout);
      if (json.target_directory) {
        return join(json.target_directory, "release");
      }
    } catch {
      /* ignore */
    }
  }
  return null;
}

function releaseCandidates() {
  const list = [];
  const fromMeta = cargoMetadataTarget();
  if (fromMeta) list.push(fromMeta);
  list.push(join(kitRoot, "target", "release"));
  list.push(join(appDir, "src-tauri", "target", "release"));
  return [...new Set(list)];
}

function findReleaseDir() {
  for (const dir of releaseCandidates()) {
    if (existsSync(join(dir, "devkit-ui.exe"))) {
      return dir;
    }
  }
  for (const dir of releaseCandidates()) {
    if (existsSync(dir)) {
      return dir;
    }
  }
  return join(kitRoot, "target", "release");
}

function copyOne(src, destName) {
  if (!existsSync(src)) {
    console.warn(`dist: no encontré ${src}`);
    return false;
  }
  const dest = join(distDir, destName);
  copyFileSync(src, dest);
  console.log(`dist: ${destName}`);
  return true;
}

mkdirSync(distDir, { recursive: true });
const release = findReleaseDir();
console.log(`dist: origen ${release}`);

copyOne(join(release, "devkit-ui.exe"), "DevKit.exe");
copyOne(join(release, "devkit-mcp.exe"), "devkit-mcp.exe");

const nsis = join(release, "bundle", "nsis");
if (existsSync(nsis)) {
  const setup = readdirSync(nsis).find((n) =>
    n.toLowerCase().endsWith("-setup.exe")
  );
  if (setup) {
    copyOne(join(nsis, setup), setup);
  } else {
    console.warn("dist: no hay instalador NSIS en bundle/nsis");
  }
} else {
  console.warn(`dist: no existe ${nsis}`);
}

console.log(`dist: listo en ${distDir}`);

import { cpSync, existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const root = resolve(appDir, "..");
const destRoot = resolve(appDir, "src-tauri", "resources", "stack");

function phpVersion() {
  const cfg = join(root, "etc", "devkit", "config.toml");
  if (!existsSync(cfg)) return "8.3";
  const text = readFileSync(cfg, "utf8");
  const match = text.match(/^\s*php_version\s*=\s*"([^"]+)"/m);
  return match?.[1] || "8.3";
}

function requireDir(path, label) {
  if (!existsSync(path)) {
    console.error(
      `No está ${label} en ${path}. Ejecuta devkit setup antes de empaquetar.`
    );
    process.exit(1);
  }
}

function copyTree(src, dest) {
  cpSync(src, dest, {
    recursive: true,
    filter: (source) => !source.toLowerCase().endsWith(".pdb"),
  });
}

const version = phpVersion();
const nginx = join(root, "bin", "nginx");
const php = join(root, "bin", "php", version);
const mariadb = join(root, "bin", "mariadb");
const composer = join(root, "bin", "composer", "composer.phar");
const node = join(root, "bin", "node");
const git = join(root, "bin", "git");
const adminer = join(root, "www", "adminer", "index.php");

function gitCmdDir() {
  const cmd = join(git, "cmd", "git.exe");
  if (existsSync(cmd)) return join(git, "cmd");
  const bin = join(git, "bin", "git.exe");
  if (existsSync(bin)) return join(git, "bin");
  const flat = join(git, "git.exe");
  if (existsSync(flat)) return git;
  return null;
}

requireDir(join(nginx, "nginx.exe"), "nginx.exe");
requireDir(join(php, "php-cgi.exe"), `php-cgi.exe (${version})`);
requireDir(join(mariadb, "bin", "mysqld.exe"), "mysqld.exe");
requireDir(composer, "composer.phar");
requireDir(join(node, "node.exe"), "node.exe");
requireDir(join(node, "npm.cmd"), "npm.cmd");
const gitDir = gitCmdDir();
if (!gitDir) {
  console.error(
    `No está git.exe en ${git}. Ejecuta devkit setup antes de empaquetar.`
  );
  process.exit(1);
}
requireDir(adminer, "adminer (www/adminer/index.php)");

rmSync(destRoot, { recursive: true, force: true });
mkdirSync(destRoot, { recursive: true });
copyTree(nginx, join(destRoot, "bin", "nginx"));
copyTree(php, join(destRoot, "bin", "php", version));
copyTree(mariadb, join(destRoot, "bin", "mariadb"));
copyTree(node, join(destRoot, "bin", "node"));
copyTree(git, join(destRoot, "bin", "git"));
mkdirSync(join(destRoot, "bin", "composer"), { recursive: true });
cpSync(composer, join(destRoot, "bin", "composer", "composer.phar"));
mkdirSync(join(destRoot, "www", "adminer"), { recursive: true });
cpSync(adminer, join(destRoot, "www", "adminer", "index.php"));

console.log(
  `Stack básico completo (PHP ${version}, Node, Git, sin PDB) en ${destRoot}`
);

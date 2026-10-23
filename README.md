# DevKit

**English** · [Español](README.es.md)

DevKit is a portable local web stack for **Windows**: Nginx, PHP (FastCGI), and MariaDB, bound to **127.0.0.1** by default. It ships a **CLI** (`devkit`) and an optional **desktop panel** (Tauri) with a system tray to start services, manage sites, and edit settings.

Use it to run PHP apps and frameworks on your machine without wiring Nginx, PHP, and MySQL yourself. Binaries are downloaded from official sources during `devkit setup` (HTTPS + SHA-256 verification), not vendored in git.

## What you get

- **HTTP/HTTPS** via Nginx (default HTTP port `8080`, HTTPS `8443` after SSL setup)
- **Multiple PHP versions** (8.3, 8.4, 8.5) under `bin/php/`; switch with `devkit php use`
- **MariaDB** for local databases; credentials stored under `etc/devkit/secrets/`
- **Optional** Redis, Mailpit, mkcert, Node, and Composer with `devkit setup --full`
- **Sites** as folders in `www/` → `https://name.<tld>` style hostnames (default TLD `test`)
- **Security-focused PHP installs**: downloads must meet current **php2shell** patch levels; `php.ini` includes recommended mitigations

## Directory layout

| Path | Purpose |
|------|---------|
| `app/` | Desktop panel (Tauri + React) |
| `bin/` | Nginx, PHP, MariaDB, Redis, Node, etc. (not in git) |
| `etc/` | Generated configs; copy `etc/devkit/config.example.toml` → `config.toml` |
| `data/` | MariaDB / Redis data |
| `www/` | Your projects (one folder per site) |
| `logs/` | Service and PHP error logs |

DevKit resolves its root from `.devkit-root`, `DEVKIT_ROOT`, or the install location (often `C:\DevKit`).

## Quick start

### 1. Build the CLI (from this repo)

```text
cargo build -p devkit-cli --release
```

The binary is `target\release\devkit.exe`. Add it to your PATH or run it from there.

### 2. Install the stack

```text
devkit setup
devkit setup --full          # Redis, Mailpit, mkcert; installs PHP 8.3, 8.4, and 8.5
devkit setup --php 8.5       # Use a specific major as the active version
```

### 3. Start and check

```text
devkit start
devkit status
```

Open `http://127.0.0.1:8080/` (or the HTTP port in `config.toml`).

### 4. Desktop panel (optional)

```text
cd app
npm install
npm run tauri dev
```

Production build: `cd app && npm run tauri build`.

In **Settings → General → Startup** you can enable: launch with Windows, start hidden in the tray, and auto-start services when the panel opens.

## CLI reference

```text
devkit setup [--full] [--php 8.4|8.5]
devkit start | stop | status | ports | reload
devkit php list | devkit php use 8.4
devkit create laravel my-shop
devkit create wordpress my-blog
devkit create blank my-api
devkit ssl
devkit pack
devkit terminal
devkit db status | devkit db sync
devkit logs list | tail <id> | search <id> <query>
```

During development from source:

```text
cargo run -p devkit-cli -- setup --full
cargo run -p devkit-cli -- start
```

## Optional: logs MCP

For Cursor / Claude Desktop, see [docs/mcp-logs.md](docs/mcp-logs.md).

## Security notes

- Services listen on **localhost** unless you change `bind` in config (not recommended for untrusted networks).
- MariaDB `root` password lives in `etc/devkit/secrets/` with restricted ACLs; default is empty password for local use only.
- Do not run project code from `www/` as Administrator.
- Keep PHP updated via `devkit setup` when security advisories apply.

## License

DevKit is released under the **[MIT License](LICENSE)**.

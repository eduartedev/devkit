# DevKit

Entorno local portable para Windows (estilo Laragon): Nginx + PHP + MariaDB en `C:\DevKit`, HTTP en **127.0.0.1:8080**.

No copia binarios de Laragon. `devkit setup` los baja de sitios oficiales y comprueba SHA-256.

## Carpetas

| Ruta | Qué es |
|------|--------|
| `app/` | Panel Tauri (bandeja) |
| `bin/` | Nginx, PHP, MariaDB, Redis, Node… (no git) |
| `etc/` | Configs generadas |
| `data/` | Datos de MariaDB/Redis |
| `www/` | Tus proyectos (una carpeta = `nombre.test:8080`) |
| `logs/` | Logs |

## CLI

```text
cargo run -p devkit-cli -- setup --full
cargo run -p devkit-cli -- start
cargo run -p devkit-cli -- status
cargo run -p devkit-cli -- stop
```

Tras el build: `C:\DevKit\target\release\devkit.exe`

- `devkit setup [--full] [--php 8.4]`
- `devkit start` / `stop` / `status` / `reload`
- `devkit php list` / `devkit php use 8.4`
- `devkit create laravel tienda`
- `devkit create wordpress blog`
- `devkit ssl`
- `devkit pack`
- `devkit terminal`

## Seguridad

Servicios atados a **127.0.0.1**. Password root de MariaDB en `etc/devkit/secrets/` (ACL restringida). No ejecutes proyectos de `www` como administrador.

## Panel

```text
cd app
npm install
npm run tauri dev
```

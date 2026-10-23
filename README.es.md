# DevKit

**Español** · [English](README.md)

DevKit es un **stack web local portable para Windows**: Nginx, PHP (FastCGI) y MariaDB, enlazado por defecto a **127.0.0.1**. Incluye una **CLI** (`devkit`) y un **panel de escritorio** opcional (Tauri) con bandeja del sistema para arrancar servicios, gestionar sitios y cambiar ajustes.

Sirve para desarrollar y probar aplicaciones PHP en tu PC sin montar Nginx, PHP y MySQL a mano. Los binarios se descargan de fuentes oficiales con `devkit setup` (HTTPS y comprobación SHA-256); no van en el repositorio git.

## Qué incluye

- **HTTP/HTTPS** con Nginx (puerto HTTP por defecto `8080`, HTTPS `8443` tras configurar SSL)
- **Varias versiones de PHP** (8.3, 8.4, 8.5) en `bin/php/`; cambia con `devkit php use`
- **MariaDB** para bases locales; credenciales en `etc/devkit/secrets/`
- **Opcional** Redis, Mailpit, mkcert, Node y Composer con `devkit setup --full`
- **Sitios** como carpetas en `www/` → hostnames `nombre.<tld>` (TLD por defecto `test`)
- **PHP con criterios de seguridad**: las descargas deben cumplir los parches mínimos de **php2shell**; el `php.ini` aplica mitigaciones recomendadas

## Carpetas

| Ruta | Uso |
|------|-----|
| `app/` | Panel de escritorio (Tauri + React) |
| `bin/` | Nginx, PHP, MariaDB, Redis, Node, etc. (no en git) |
| `etc/` | Configs generadas; copia `etc/devkit/config.example.toml` → `config.toml` |
| `data/` | Datos de MariaDB / Redis |
| `www/` | Tus proyectos (una carpeta = un sitio) |
| `logs/` | Logs de servicios y errores PHP |

DevKit detecta la raíz con `.devkit-root`, `DEVKIT_ROOT` o la ruta de instalación (a menudo `C:\DevKit`).

## Inicio rápido

### 1. Compilar la CLI (desde este repo)

```text
cargo build -p devkit-cli --release
```

El ejecutable queda en `target\release\devkit.exe`. Añádelo al PATH o ejecútalo desde ahí.

### 2. Instalar el stack

```text
devkit setup
devkit setup --full          # Redis, Mailpit, mkcert; instala PHP 8.3, 8.4 y 8.5
devkit setup --php 8.5       # Fija un major como versión activa
```

### 3. Arrancar y comprobar

```text
devkit start
devkit status
```

Abre `http://127.0.0.1:8080/` (o el puerto HTTP de `config.toml`).

### 4. Panel de escritorio (opcional)

```text
cd app
npm install
npm run tauri dev
```

Build de producción: `cd app && npm run tauri build`.

En **Configuración → General → Arranque** puedes activar: abrir con Windows, iniciar oculto en bandeja y arrancar servicios al abrir el panel.

## Referencia CLI

```text
devkit setup [--full] [--php 8.4|8.5]
devkit start | stop | status | ports | reload
devkit php list | devkit php use 8.4
devkit create laravel mi-tienda
devkit create wordpress mi-blog
devkit create blank mi-api
devkit ssl
devkit pack
devkit terminal
devkit db status | devkit db sync
devkit logs list | tail <id> | search <id> <consulta>
```

Desde el código fuente:

```text
cargo run -p devkit-cli -- setup --full
cargo run -p devkit-cli -- start
```

## MCP de logs (opcional)

Para Cursor / Claude Desktop: [docs/mcp-logs.md](docs/mcp-logs.md).

## Seguridad

- Los servicios escuchan en **localhost** salvo que cambies `bind` en la config (no recomendado en redes no confiables).
- La contraseña `root` de MariaDB está en `etc/devkit/secrets/` con ACL restringida; por defecto vacía solo para uso local.
- No ejecutes proyectos de `www/` como administrador.
- Actualiza PHP con `devkit setup` cuando haya avisos de seguridad.

## Licencia

DevKit se distribuye bajo la **[licencia MIT](LICENSE)**.

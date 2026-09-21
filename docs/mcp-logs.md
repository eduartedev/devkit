# MCP de logs DevKit (opcional)

El servidor **no** arranca con `devkit start`. Solo lo lanza Cursor, Claude Desktop u otro cliente MCP cuando lo configuras.

## Qué hace

Tools de solo lectura:

| Tool | Uso |
|------|-----|
| `devkit_logs_list` | Lista logs del kit en `logs/` y opcionalmente frameworks bajo `www/` |
| `devkit_logs_tail` | Últimas N líneas de un log |
| `devkit_logs_search` | Busca texto (Fatal, Exception, …) |

Todo el stack escribe en **`logs/`** (una sola carpeta):

| Id MCP/CLI | Archivo |
|------------|---------|
| `php` | `logs/php_error.log` |
| `nginx-error` | `logs/nginx-error.log` |
| `nginx-access` | `logs/nginx-access.log` |
| `mariadb` | `logs/mariadb-error.log` |
| `redis` | `logs/redis.log` |
| `mailpit` | `logs/mailpit.log` |

Laravel y similares siguen en su carpeta (`storage/logs/…`); el MCP también puede listarlos.

## Compilar

```powershell
cd C:\DevKit
cargo build -p devkit-mcp --release
# → target\release\devkit-mcp.exe
```

## Activar en Cursor

En la config MCP del proyecto o de usuario (ej. `.cursor/mcp.json`):

```json
{
  "mcpServers": {
    "devkit": {
      "command": "C:\\DevKit\\target\\release\\devkit-mcp.exe",
      "args": []
    }
  }
}
```

Ajusta la ruta si tu DevKit no está en `C:\DevKit`.

Reinicia MCP / Cursor. Por defecto, si no añades esta entrada, **no hay proceso MCP**.

## CLI sin MCP

```powershell
devkit logs list
devkit logs tail php --lines 100
devkit logs search php "Fatal"
devkit logs tail mitienda/storage/logs/laravel.log
```

Tras cambiar `php.ini`, reinicia PHP (`devkit stop` / `start` o reinicio del servicio PHP) para que use `logs/php_error.log`.

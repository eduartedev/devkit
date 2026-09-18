# Historias de uso — DevKit

Checklist manual (panel + CLI) y scripts smoke en `scripts/smoke/`.

## Prerrequisitos

- Binarios instalados: `devkit setup --full` (o panel → Setup completo)
- CLI: `cargo build -p devkit-cli --release` → `target\release\devkit.exe`
- Panel (opcional): `cd app && npm run tauri build` → exe de DevKit, o `npm run tauri dev` en desarrollo
- PHP/Composer disponibles dentro del kit; red para crear Laravel

---

## Historia 1 — Arrancar todos los servicios

**Como** desarrollador local  
**Quiero** iniciar el stack completo  
**Para** servir sitios en `127.0.0.1` sin pelearme con Laragon en los mismos puertos (si chocan, edito puertos).

### Checklist panel

- [ ] Abrir DevKit (exe o `tauri dev`)
- [ ] Pulsar **Start All**
- [ ] Servicios en verde / `running`: Nginx, PHP, MariaDB (Redis/Mailpit si aplica)
- [ ] Sin avisos graves de puerto ocupado por proceso ajeno
- [ ] Abrir `http://127.0.0.1:<http_port>/` (o Adminer) y ver respuesta

### Checklist CLI

```powershell
devkit start
devkit status
# Esperado: nginx/php/mariadb en running
```

### Smoke automático

```powershell
.\scripts\smoke\Invoke-SmokeStories.ps1 -Story Start
```

---

## Historia 2 — Crear Laravel desde cero y abrirlo

**Como** desarrollador  
**Quiero** crear un proyecto Laravel nuevo con DevKit  
**Para** que Composer/PHP del kit instalen bien, el vhost apunte a `public/` y la URL abra en el navegador.

### Checklist panel

- [ ] Stack en marcha (Historia 1)
- [ ] En Sitios: tipo **Laravel**, nombre válido (ej. `mitienda`)
- [ ] **Crear** — espera a que termine Composer (puede tardar)
- [ ] El sitio aparece en la lista (`mitienda.test`)
- [ ] Clic en el hostname → navegador
- [ ] Respuesta OK (página Laravel / welcome), no 403 ni 502

### Checklist CLI

```powershell
devkit start
devkit create laravel mitienda
# Abrir URL que muestre status (http o https según ssl)
start http://mitienda.test:8082/   # ajusta puerto/ssl
```

### Smoke automático

```powershell
.\scripts\smoke\Invoke-SmokeStories.ps1 -Story Laravel
# Crea www\smoke-laravel-*, hace curl y abre el navegador
# -KeepSite para no borrar el proyecto al final
```

---

## Historia 3 — Parar todos los servicios

**Como** desarrollador  
**Quiero** detener el stack  
**Para** liberar CPU/puertos sin dejar Nginx/PHP/MariaDB huérfanos de DevKit.

### Checklist panel

- [ ] Pulsar **Stop All**
- [ ] Servicios en `stopped` / apagados
- [ ] `http://127.0.0.1:<http_port>/` ya no responde (o conexión rechazada)
- [ ] (Opcional) Administrador de tareas: no quedan `nginx`/`php-cgi`/`mysqld` del kit si no usas otro stack

### Checklist CLI

```powershell
devkit stop
devkit status
# Esperado: servicios stopped
```

### Smoke automático

```powershell
.\scripts\smoke\Invoke-SmokeStories.ps1 -Story Stop
```

---

## Las tres seguidas

```powershell
.\scripts\smoke\Invoke-SmokeStories.ps1 -Story All
```

Orden: Start → Laravel → Stop.

use crate::bind::require_localhost;
use crate::error::{Error, Result};
use crate::health::{is_port_open, wait_until_closed, wait_until_open};
use crate::model::{ServiceSnapshot, ServiceSpec, ServiceStatus, StopKind};
use crate::ports::{check_service_port, PortOwner};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
#[cfg(windows)]
const DETACHED_PROCESS: u32 = 0x0000_0008;

struct Live {
    child: Option<Child>,
    #[allow(dead_code)]
    spec: ServiceSpec,
}

pub struct Supervisor {
    inner: Mutex<HashMap<String, Live>>,
}

impl Default for Supervisor {
    fn default() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

impl Supervisor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self, spec: &ServiceSpec) -> ServiceSnapshot {
        // Ligero: sin netstat/tasklist (el panel llama esto cada pocos segundos).
        let owner = crate::ports::classify_port_light(&spec.bind_host, spec.port, &spec.pidfile);
        let pid = match &owner {
            PortOwner::Ours { pid, .. } => Some(*pid),
            _ => read_pid(&spec.pidfile).filter(|p| crate::ports::process_alive_fast(*p)),
        };
        let (status, detail) = match &owner {
            PortOwner::Ours { pid, .. } => (
                ServiceStatus::Running,
                format!("escucha {}:{} · PID {pid}", spec.bind_host, spec.port),
            ),
            PortOwner::Unknown => (
                ServiceStatus::Stopped,
                format!(
                    "puerto {} ocupado (pulsa Revisar puertos para ver quién)",
                    spec.port
                ),
            ),
            PortOwner::Foreign { processes } => {
                let who = processes
                    .iter()
                    .map(|p| p.describe())
                    .collect::<Vec<_>>()
                    .join(", ");
                (
                    ServiceStatus::Stopped,
                    format!("puerto {} ocupado por {who}", spec.port),
                )
            }
            PortOwner::Free => {
                if pid.is_some() {
                    (
                        ServiceStatus::Crashed,
                        "pidfile presente pero el puerto no responde".into(),
                    )
                } else {
                    (ServiceStatus::Stopped, "detenido".into())
                }
            }
        };
        ServiceSnapshot {
            id: spec.id.clone(),
            display_name: spec.display_name.clone(),
            status,
            bind: spec.bind_host.clone(),
            port: spec.port,
            pid,
            detail,
        }
    }

    pub fn start(&self, spec: ServiceSpec, detach: bool) -> Result<ServiceSnapshot> {
        require_localhost(&spec.bind_host)?;
        if !spec.binary.exists() {
            return Err(Error::MissingBinary {
                path: spec.binary.clone(),
            });
        }

        let check = check_service_port(
            &spec.id,
            &spec.display_name,
            &spec.bind_host,
            spec.port,
            &spec.pidfile,
        );
        match &check.owner {
            PortOwner::Ours { .. } => {
                let snap = self.snapshot(&spec);
                return Ok(ServiceSnapshot {
                    detail: "ya estaba en marcha".into(),
                    ..snap
                });
            }
            PortOwner::Foreign { processes } => {
                let who = if processes.is_empty() {
                    "proceso desconocido".into()
                } else {
                    processes
                        .iter()
                        .map(|p| p.describe())
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let alts = if check.suggested_ports.is_empty() {
                    String::new()
                } else {
                    format!(
                        "; puertos libres sugeridos: {}",
                        check
                            .suggested_ports
                            .iter()
                            .map(|p| p.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                return Err(Error::PortOccupied {
                    service: spec.display_name.clone(),
                    port: spec.port,
                    hint: format!("{who}. {}{alts}", check.hint),
                });
            }
            PortOwner::Unknown => {
                let alts = if check.suggested_ports.is_empty() {
                    String::new()
                } else {
                    format!(
                        "; puertos libres sugeridos: {}",
                        check
                            .suggested_ports
                            .iter()
                            .map(|p| p.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                return Err(Error::PortOccupied {
                    service: spec.display_name.clone(),
                    port: spec.port,
                    hint: format!("proceso no identificado. {}{alts}", check.hint),
                });
            }
            PortOwner::Free => {}
        }

        let mut cmd = spec.command();
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            let mut flags = CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP;
            if detach {
                flags |= DETACHED_PROCESS;
            }
            cmd.creation_flags(flags);
        }

        // Quita herencia de pipes del shell mientras spawneamos (ver `win_spawn`).
        let _inherit_guard = crate::win_spawn::StdioInheritGuard::clear();
        let child = cmd.spawn().map_err(|e| {
            Error::msg(format!(
                "no se pudo arrancar {}: {e}",
                spec.display_name
            ))
        })?;
        drop(_inherit_guard);
        let pid = child.id();
        if let Some(dir) = spec.pidfile.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&spec.pidfile, pid.to_string())?;

        {
            let mut map = self.inner.lock();
            if detach {
                std::mem::forget(child);
                map.insert(
                    spec.id.clone(),
                    Live {
                        child: None,
                        spec: spec.clone(),
                    },
                );
            } else {
                map.insert(
                    spec.id.clone(),
                    Live {
                        child: Some(child),
                        spec: spec.clone(),
                    },
                );
            }
        }

        if !wait_until_open(&spec.bind_host, spec.port, 80) {
            return Ok(ServiceSnapshot {
                id: spec.id.clone(),
                display_name: spec.display_name.clone(),
                status: ServiceStatus::Crashed,
                bind: spec.bind_host.clone(),
                port: spec.port,
                pid: Some(pid),
                detail: "el proceso arrancó pero el puerto no responde".into(),
            });
        }
        Ok(self.snapshot(&spec))
    }

    pub fn stop(&self, spec: &ServiceSpec) -> Result<ServiceSnapshot> {
        match spec.stop {
            StopKind::NginxQuit => {
                // Debe usar el mismo -p/-c con el que arrancó; si no, quit no llega al master.
                let mut cmd = Command::new(&spec.binary);
                cmd.args(&spec.args).args(["-s", "quit"]);
                if let Some(cwd) = &spec.cwd {
                    cmd.current_dir(cwd);
                }
                let _ = cmd.status();
                // stop es más agresivo si quit no cierra a tiempo.
                let mut cmd = Command::new(&spec.binary);
                cmd.args(&spec.args).args(["-s", "stop"]);
                if let Some(cwd) = &spec.cwd {
                    cmd.current_dir(cwd);
                }
                let _ = cmd.status();
            }
            StopKind::MysqlAdmin => {
                // Preferir stop_mysql_admin desde kit; aquí solo fuerza después.
            }
            StopKind::RedisShutdown => {
                if let Ok(mut stream) =
                    TcpStream::connect_timeout(&crate::health::local_addr(spec.port), Duration::from_millis(400))
                {
                    let _ = stream.write_all(b"SHUTDOWN NOSAVE\r\n");
                    let mut buf = [0u8; 32];
                    let _ = stream.read(&mut buf);
                }
            }
            StopKind::Terminate => {}
        }

        if let Some(pid) = read_pid(&spec.pidfile) {
            kill_pid(pid);
        }

        {
            let mut map = self.inner.lock();
            if let Some(mut live) = map.remove(&spec.id) {
                if let Some(mut child) = live.child.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }

        // Segunda pasada: lo que siga escuchando el puerto con nombre del servicio.
        kill_listeners_for_service(spec);
        wait_until_closed(&spec.bind_host, spec.port, 40);
        if is_port_open(&spec.bind_host, spec.port) {
            kill_listeners_for_service(spec);
            wait_until_closed(&spec.bind_host, spec.port, 20);
        }
        let _ = fs::remove_file(&spec.pidfile);
        // Nginx escribe también logs/nginx.pid (master).
        if spec.id == "nginx" {
            if let Some(parent) = spec.pidfile.parent() {
                let _ = fs::remove_file(parent.join("nginx.pid"));
            }
        }
        Ok(self.snapshot(spec))
    }

    pub fn stop_mysql_admin(
        &self,
        spec: &ServiceSpec,
        mysqladmin: &PathBuf,
        user: &str,
        password: &str,
    ) -> Result<ServiceSnapshot> {
        if mysqladmin.exists() && crate::health::is_port_open(&spec.bind_host, spec.port) {
            let try_shutdown = |user: &str, password: &str| {
                let mut args = crate::secrets::mysql_cli_auth_args(user, password);
                args.extend([
                    "-h".into(),
                    "127.0.0.1".into(),
                    "-P".into(),
                    spec.port.to_string(),
                    "shutdown".into(),
                ]);
                let _guard = crate::win_spawn::StdioInheritGuard::clear();
                let ok = Command::new(mysqladmin)
                    .args(&args)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                drop(_guard);
                ok
            };
            if !try_shutdown(user, password) && !password.is_empty() {
                let _ = try_shutdown(user, "");
            }
            // Espera breve al cierre limpio antes del taskkill.
            let _ = wait_until_closed(&spec.bind_host, spec.port, 20);
        }
        self.stop(spec)
    }
}

fn read_pid(path: &PathBuf) -> Option<u32> {
    let text = fs::read_to_string(path).ok()?;
    text.trim().parse().ok()
}

fn expected_process_names(service_id: &str) -> &'static [&'static str] {
    match service_id {
        "nginx" => &["nginx"],
        "php" => &["php-cgi", "php"],
        "mariadb" => &["mysqld", "mariadbd", "mariadb"],
        "redis" => &["redis-server", "redis"],
        "mailpit" => &["mailpit"],
        _ => &[],
    }
}

fn kill_listeners_for_service(spec: &ServiceSpec) {
    let names = expected_process_names(&spec.id);
    if names.is_empty() {
        return;
    }
    for proc in crate::ports::find_listeners(spec.port) {
        let n = proc.name.to_ascii_lowercase();
        if names.iter().any(|want| n.contains(want)) {
            kill_pid(proc.pid);
        }
    }
}

fn kill_pid(pid: u32) {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::LOCALHOST;
    use tempfile::tempdir;

    fn powershell_exe() -> PathBuf {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
        PathBuf::from(windir).join(r"System32\WindowsPowerShell\v1.0\powershell.exe")
    }

    #[test]
    fn rejects_non_localhost_bind() {
        let dir = tempdir().unwrap();
        let spec = ServiceSpec {
            id: "bad".into(),
            display_name: "bad".into(),
            binary: powershell_exe(),
            args: vec![],
            env: vec![],
            cwd: None,
            bind_host: "0.0.0.0".into(),
            port: 1,
            pidfile: dir.path().join("x.pid"),
            stop: StopKind::Terminate,
        };
        let sup = Supervisor::new();
        let err = sup.start(spec, true).unwrap_err();
        match err {
            Error::BindForbidden { .. } => {}
            other => panic!("esperado BindForbidden, got {other}"),
        }
    }

    #[test]
    fn occupied_port_is_error_for_other_binary() {
        let dir = tempdir().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let spec = ServiceSpec {
            id: "nginx".into(),
            display_name: "Nginx".into(),
            binary: powershell_exe(),
            args: vec!["-NoProfile".into(), "-Command".into(), "Start-Sleep 1".into()],
            env: vec![],
            cwd: None,
            bind_host: LOCALHOST.into(),
            port,
            pidfile: dir.path().join("n.pid"),
            stop: StopKind::Terminate,
        };
        let sup = Supervisor::new();
        let err = sup.start(spec, true).unwrap_err();
        match err {
            Error::PortOccupied { port: p, .. } => assert_eq!(p, port),
            other => panic!("esperado PortOccupied, got {other}"),
        }
    }
}

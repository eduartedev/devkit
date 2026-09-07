use crate::error::{Error, Result};
use crate::model::LOCALHOST;
use crate::paths::AppConfig;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Puertos editables desde el panel (mirror de `AppConfig`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortsSettings {
    pub http_port: u16,
    pub https_port: u16,
    pub php_port: u16,
    pub mysql_port: u16,
    pub redis_port: u16,
    pub mailpit_smtp: u16,
    pub mailpit_ui: u16,
}

impl PortsSettings {
    pub fn from_config(cfg: &AppConfig) -> Self {
        Self {
            http_port: cfg.http_port,
            https_port: cfg.https_port,
            php_port: cfg.php_port,
            mysql_port: cfg.mysql_port,
            redis_port: cfg.redis_port,
            mailpit_smtp: cfg.mailpit_smtp,
            mailpit_ui: cfg.mailpit_ui,
        }
    }

    pub fn apply_to(&self, cfg: &mut AppConfig) {
        cfg.http_port = self.http_port;
        cfg.https_port = self.https_port;
        cfg.php_port = self.php_port;
        cfg.mysql_port = self.mysql_port;
        cfg.redis_port = self.redis_port;
        cfg.mailpit_smtp = self.mailpit_smtp;
        cfg.mailpit_ui = self.mailpit_ui;
    }

    /// Rechaza puerto 0 y valores duplicados entre servicios DevKit.
    pub fn validate(&self) -> Result<()> {
        let labeled = [
            ("HTTP", self.http_port),
            ("HTTPS", self.https_port),
            ("PHP", self.php_port),
            ("MariaDB", self.mysql_port),
            ("Redis", self.redis_port),
            ("Mailpit SMTP", self.mailpit_smtp),
            ("Mailpit UI", self.mailpit_ui),
        ];
        for (name, port) in labeled {
            if port == 0 {
                return Err(Error::msg(format!(
                    "puerto inválido para {name}: 0 (usa 1–65535)"
                )));
            }
        }
        let mut seen: HashMap<u16, &'static str> = HashMap::new();
        for (name, port) in labeled {
            if let Some(other) = seen.insert(port, name) {
                return Err(Error::msg(format!(
                    "puerto {port} duplicado entre {other} y {name}"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortProcess {
    pub pid: u32,
    pub name: String,
}

impl PortProcess {
    pub fn describe(&self) -> String {
        format!("{} (PID {})", self.name, self.pid)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PortOwner {
    Free,
    /// Nuestro servicio (pidfile vivo y escuchando).
    Ours { pid: u32, name: String },
    /// Otro proceso (Laragon, IIS, etc.).
    Foreign { processes: Vec<PortProcess> },
    /// El puerto responde pero no pudimos resolver el PID.
    Unknown,
}

impl PortOwner {
    pub fn is_blocking(&self) -> bool {
        matches!(self, Self::Foreign { .. } | Self::Unknown)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortCheck {
    pub service: String,
    pub display_name: String,
    pub bind: String,
    pub port: u16,
    pub owner: PortOwner,
    pub hint: String,
    pub suggested_ports: Vec<u16>,
}

impl PortCheck {
    pub fn is_conflict(&self) -> bool {
        self.owner.is_blocking()
    }

    pub fn summary(&self) -> String {
        match &self.owner {
            PortOwner::Free => format!("{} :{} libre", self.display_name, self.port),
            PortOwner::Ours { pid, name } => format!(
                "{} :{} ya es DevKit ({name}, PID {pid})",
                self.display_name, self.port
            ),
            PortOwner::Foreign { processes } => {
                let who = if processes.is_empty() {
                    "proceso desconocido".into()
                } else {
                    processes
                        .iter()
                        .map(PortProcess::describe)
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let alts = if self.suggested_ports.is_empty() {
                    String::new()
                } else {
                    format!(
                        " · alternativas: {}",
                        self.suggested_ports
                            .iter()
                            .map(|p| p.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                format!(
                    "{} :{} ocupado por {who}{alts}",
                    self.display_name, self.port
                )
            }
            PortOwner::Unknown => format!(
                "{} :{} responde pero no se identificó el proceso",
                self.display_name, self.port
            ),
        }
    }
}

/// Tabla de un solo `netstat` (puerto → PIDs).
#[derive(Debug, Default, Clone)]
pub struct ListeningTable {
    by_port: HashMap<u16, Vec<u32>>,
    names: HashMap<u32, String>,
}

impl ListeningTable {
    pub fn capture() -> Self {
        let mut table = Self::default();
        let output = Command::new("netstat")
            .args(["-ano", "-p", "tcp"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();
        let Ok(out) = output else {
            return table;
        };
        if !out.status.success() {
            return table;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        table.by_port = parse_netstat_all_listening(&text);
        let mut pids: Vec<u32> = table.by_port.values().flatten().copied().collect();
        pids.sort_unstable();
        pids.dedup();
        for pid in pids {
            if let Some(name) = process_name(pid) {
                table.names.insert(pid, name);
            }
        }
        table
    }

    pub fn listeners_for(&self, port: u16) -> Vec<PortProcess> {
        self.by_port
            .get(&port)
            .into_iter()
            .flatten()
            .map(|pid| PortProcess {
                pid: *pid,
                name: self
                    .names
                    .get(pid)
                    .cloned()
                    .unwrap_or_else(|| "unknown".into()),
            })
            .collect()
    }
}

struct PortCache {
    at: Instant,
    checks: Vec<PortCheck>,
}

fn port_cache() -> &'static Mutex<Option<PortCache>> {
    static CACHE: OnceLock<Mutex<Option<PortCache>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

pub fn cached_port_checks() -> Option<Vec<PortCheck>> {
    let guard = port_cache().lock();
    guard.as_ref().map(|c| c.checks.clone())
}

pub fn cache_port_checks(checks: Vec<PortCheck>) {
    *port_cache().lock() = Some(PortCache {
        at: Instant::now(),
        checks,
    });
}

pub fn clear_port_cache() {
    *port_cache().lock() = None;
}

pub fn cached_port_checks_fresh(max_age: Duration) -> Option<Vec<PortCheck>> {
    let guard = port_cache().lock();
    guard.as_ref().and_then(|c| {
        if c.at.elapsed() <= max_age {
            Some(c.checks.clone())
        } else {
            None
        }
    })
}

/// Clasificación ligera: solo TCP + pidfile (sin netstat). Ideal para el panel.
pub fn classify_port_light(host: &str, port: u16, pidfile: &Path) -> PortOwner {
    let our_pid = read_pidfile(pidfile);
    if !crate::health::is_port_open(host, port) {
        if let Some(pid) = our_pid {
            if !process_alive_fast(pid) {
                let _ = fs::remove_file(pidfile);
            }
        }
        return PortOwner::Free;
    }
    if let Some(our) = our_pid {
        if process_alive_fast(our) {
            return PortOwner::Ours {
                pid: our,
                name: "devkit-service".into(),
            };
        }
        let _ = fs::remove_file(pidfile);
    }
    // Puerto abierto sin nuestro pidfile: conflicto probable (detalle en check_ports).
    PortOwner::Unknown
}

/// Clasifica un puerto respecto a nuestro pidfile (con tabla netstat opcional).
pub fn classify_port(host: &str, port: u16, pidfile: &Path) -> PortOwner {
    let table = ListeningTable::capture();
    classify_port_with_table(host, port, pidfile, &table)
}

pub fn classify_port_with_table(
    host: &str,
    port: u16,
    pidfile: &Path,
    table: &ListeningTable,
) -> PortOwner {
    if !crate::health::is_port_open(host, port) {
        if let Some(pid) = read_pidfile(pidfile) {
            if !process_alive_fast(pid) {
                let _ = fs::remove_file(pidfile);
            }
        }
        return PortOwner::Free;
    }

    let listeners = table.listeners_for(port);
    let our_pid = read_pidfile(pidfile).filter(|pid| process_alive_fast(*pid));

    if let Some(our) = our_pid {
        if listeners.iter().any(|p| p.pid == our) || listeners.is_empty() {
            let name = table
                .names
                .get(&our)
                .cloned()
                .or_else(|| process_name(our))
                .unwrap_or_else(|| "devkit-service".into());
            return PortOwner::Ours { pid: our, name };
        }
    }

    if listeners.is_empty() {
        return PortOwner::Unknown;
    }

    if our_pid.is_none() {
        let _ = fs::remove_file(pidfile);
    }

    PortOwner::Foreign {
        processes: listeners,
    }
}

pub fn check_service_port(
    service: &str,
    display_name: &str,
    host: &str,
    port: u16,
    pidfile: &Path,
) -> PortCheck {
    let table = ListeningTable::capture();
    check_service_port_with_table(service, display_name, host, port, pidfile, &table, true)
}

pub fn check_service_port_with_table(
    service: &str,
    display_name: &str,
    host: &str,
    port: u16,
    pidfile: &Path,
    table: &ListeningTable,
    suggest: bool,
) -> PortCheck {
    let host = if host.eq_ignore_ascii_case("localhost") {
        LOCALHOST
    } else {
        host
    };
    let owner = classify_port_with_table(host, port, pidfile, table);
    let suggested = if suggest && owner.is_blocking() {
        suggest_alternates(port)
    } else {
        Vec::new()
    };
    let hint = match &owner {
        PortOwner::Free => "disponible".into(),
        PortOwner::Ours { .. } => "DevKit ya lo tiene en marcha".into(),
        PortOwner::Foreign { processes } => {
            let names: Vec<_> = processes.iter().map(|p| p.name.to_ascii_lowercase()).collect();
            let known = guess_stack(&names);
            if known.is_empty() {
                "apaga ese proceso o cambia el puerto en etc/devkit/config.toml".into()
            } else {
                format!(
                    "parece {known}; apágalo o cambia el puerto en etc/devkit/config.toml"
                )
            }
        }
        PortOwner::Unknown => "revisa netstat / Resource Monitor".into(),
    };
    PortCheck {
        service: service.into(),
        display_name: display_name.into(),
        bind: host.into(),
        port,
        owner,
        hint,
        suggested_ports: suggested,
    }
}

/// Un solo netstat para todos los servicios (uso en start/ports).
pub fn check_many(
    specs: &[(String, String, String, u16, std::path::PathBuf)],
) -> Vec<PortCheck> {
    let table = ListeningTable::capture();
    let mut out = Vec::with_capacity(specs.len());
    for (id, display, host, port, pidfile) in specs {
        out.push(check_service_port_with_table(
            id, display, host, *port, pidfile, &table, true,
        ));
    }
    cache_port_checks(out.clone());
    out
}

pub fn suggest_alternates(port: u16) -> Vec<u16> {
    let candidates: &[u16] = match port {
        8080 => &[8081, 8088, 8888],
        8443 => &[8444, 9443],
        9000 => &[9001, 9002, 9070],
        3306 => &[3307, 3308, 3310],
        6379 => &[6380, 6381],
        8025 => &[8026, 8027],
        1025 => &[1026, 2525],
        53 => &[5353],
        _ => &[port.saturating_add(1), port.saturating_add(2)],
    };
    candidates
        .iter()
        .copied()
        .filter(|p| !crate::health::is_port_open(LOCALHOST, *p))
        .take(3)
        .collect()
}

fn guess_stack(names: &[String]) -> String {
    let joined = names.join(" ");
    if joined.contains("mysqld") || joined.contains("mariadbd") || joined.contains("mysql") {
        return "Laragon/XAMPP/MySQL".into();
    }
    if joined.contains("httpd") || joined.contains("apache") {
        return "Apache (Laragon/XAMPP)".into();
    }
    if joined.contains("nginx") {
        return "otro Nginx".into();
    }
    if joined.contains("redis") {
        return "otro Redis".into();
    }
    if joined.contains("php-cgi") || joined.contains("php-fpm") {
        return "otro PHP FastCGI".into();
    }
    if joined.contains("iis") || joined.contains("w3wp") {
        return "IIS".into();
    }
    String::new()
}

pub fn find_listeners(port: u16) -> Vec<PortProcess> {
    ListeningTable::capture().listeners_for(port)
}

/// Extrae PIDs en estado LISTENING para `port` desde salida de `netstat -ano`.
pub fn parse_netstat_listening_pids(netstat: &str, port: u16) -> Vec<u32> {
    parse_netstat_all_listening(netstat)
        .remove(&port)
        .unwrap_or_default()
}

pub fn parse_netstat_all_listening(netstat: &str) -> HashMap<u16, Vec<u32>> {
    let mut map: HashMap<u16, Vec<u32>> = HashMap::new();
    for line in netstat.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let upper = line.to_ascii_uppercase();
        if !upper.contains("LISTENING") && !upper.contains("LISTEN") {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }
        let local = parts[1];
        let Some(port) = parse_local_port(local) else {
            continue;
        };
        if let Ok(pid) = parts[parts.len() - 1].parse::<u32>() {
            if pid > 0 {
                map.entry(port).or_default().push(pid);
            }
        }
    }
    for v in map.values_mut() {
        v.sort_unstable();
        v.dedup();
    }
    map
}

fn parse_local_port(local: &str) -> Option<u16> {
    let idx = local.rfind(':')?;
    local[idx + 1..].parse().ok()
}

pub fn read_pidfile(path: &Path) -> Option<u32> {
    let text = fs::read_to_string(path).ok()?;
    text.trim().parse().ok()
}

/// Comprueba si el PID existe sin lanzar tasklist (más barato en el panel).
pub fn process_alive_fast(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
        unsafe {
            match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                Ok(handle) => {
                    let _ = CloseHandle(handle);
                    true
                }
                Err(_) => false,
            }
        }
    }
    #[cfg(not(windows))]
    {
        Path::new(&format!("/proc/{pid}")).exists()
    }
}

pub fn process_alive(pid: u32) -> bool {
    process_alive_fast(pid)
}

pub fn process_name(pid: u32) -> Option<String> {
    #[cfg(windows)]
    {
        let out = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        parse_tasklist_name(&text, pid)
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        None
    }
}

pub fn parse_tasklist_name(csv_line: &str, pid: u32) -> Option<String> {
    for line in csv_line.lines() {
        let line = line.trim();
        if line.is_empty() || line.to_ascii_uppercase().starts_with("INFO:") {
            continue;
        }
        let cols: Vec<&str> = line.split(',').collect();
        if cols.len() < 2 {
            continue;
        }
        let name = cols[0].trim().trim_matches('"');
        let pid_col = cols[1].trim().trim_matches('"');
        if pid_col.parse::<u32>().ok() == Some(pid) {
            return Some(name.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::is_port_open;
    use std::net::TcpListener;
    use tempfile::tempdir;

    #[test]
    fn parses_netstat_listening_line() {
        let sample = r#"
  Active Connections

  Proto  Local Address          Foreign Address        State           PID
  TCP    0.0.0.0:135            0.0.0.0:0              LISTENING       1232
  TCP    127.0.0.1:3306         0.0.0.0:0              LISTENING       4456
  TCP    127.0.0.1:33060        0.0.0.0:0              LISTENING       9999
  TCP    [::1]:8080             [::]:0                 LISTENING       7001
"#;
        let pids = parse_netstat_listening_pids(sample, 3306);
        assert_eq!(pids, vec![4456]);
        let all = parse_netstat_all_listening(sample);
        assert_eq!(all.get(&8080), Some(&vec![7001]));
        assert!(parse_netstat_listening_pids(sample, 9999).is_empty());
    }

    #[test]
    fn parses_tasklist_csv() {
        let line = "\"mysqld.exe\",\"4456\",\"Console\",\"1\",\"50,000 K\"\r\n";
        assert_eq!(
            parse_tasklist_name(line, 4456).as_deref(),
            Some("mysqld.exe")
        );
        assert!(parse_tasklist_name(line, 1).is_none());
    }

    #[test]
    fn classify_free_when_nothing_listens() {
        let dir = tempdir().unwrap();
        let pidfile = dir.path().join("x.pid");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        assert!(!is_port_open(LOCALHOST, port));
        assert_eq!(classify_port_light(LOCALHOST, port, &pidfile), PortOwner::Free);
    }

    #[test]
    fn classify_light_unknown_without_pidfile() {
        let dir = tempdir().unwrap();
        let pidfile = dir.path().join("x.pid");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(is_port_open(LOCALHOST, port));
        let owner = classify_port_light(LOCALHOST, port, &pidfile);
        assert!(matches!(owner, PortOwner::Unknown));
        drop(listener);
    }

    #[test]
    fn classify_foreign_without_our_pidfile() {
        let dir = tempdir().unwrap();
        let pidfile = dir.path().join("x.pid");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(is_port_open(LOCALHOST, port));
        let owner = classify_port(LOCALHOST, port, &pidfile);
        match owner {
            PortOwner::Foreign { .. } | PortOwner::Unknown => {}
            other => panic!("esperado Foreign/Unknown, got {other:?}"),
        }
        drop(listener);
    }

    #[test]
    fn classify_ours_when_pidfile_matches_listener() {
        let dir = tempdir().unwrap();
        let pidfile = dir.path().join("ours.pid");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let our_pid = std::process::id();
        fs::write(&pidfile, our_pid.to_string()).unwrap();
        let owner = classify_port_light(LOCALHOST, port, &pidfile);
        assert!(matches!(owner, PortOwner::Ours { pid, .. } if pid == our_pid));
        drop(listener);
    }

    #[test]
    fn check_builds_conflict_summary() {
        let dir = tempdir().unwrap();
        let pidfile = dir.path().join("m.pid");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let check = check_service_port("mariadb", "MariaDB", LOCALHOST, port, &pidfile);
        assert!(check.is_conflict() || matches!(check.owner, PortOwner::Ours { .. }));
        let text = check.summary();
        assert!(text.contains("MariaDB"));
        assert!(text.contains(&port.to_string()));
        drop(listener);
    }

    #[test]
    fn suggests_free_alternates_for_mysql() {
        let alts = suggest_alternates(3306);
        assert!(alts.iter().all(|p| *p != 3306));
        assert!(alts.len() <= 3);
    }

    #[test]
    fn ports_settings_rejects_zero() {
        let mut p = PortsSettings::from_config(&AppConfig::default());
        p.http_port = 0;
        let err = p.validate().unwrap_err().to_string();
        assert!(err.contains("HTTP"));
        assert!(err.contains("0"));
    }

    #[test]
    fn ports_settings_rejects_duplicates() {
        let mut p = PortsSettings::from_config(&AppConfig::default());
        p.redis_port = p.mysql_port;
        let err = p.validate().unwrap_err().to_string();
        assert!(err.contains("duplicado"));
        assert!(err.contains(&p.mysql_port.to_string()));
    }

    #[test]
    fn ports_settings_saves_into_config_toml() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut cfg = AppConfig::default();
        cfg.save(&path).unwrap();

        let mut ports = PortsSettings::from_config(&cfg);
        ports.http_port = 8088;
        ports.mysql_port = 3307;
        ports.validate().unwrap();
        ports.apply_to(&mut cfg);
        cfg.save(&path).unwrap();

        let loaded = AppConfig::load(&path).unwrap();
        assert_eq!(loaded.http_port, 8088);
        assert_eq!(loaded.mysql_port, 3307);
        assert_eq!(loaded.php_port, 9000);
    }
}

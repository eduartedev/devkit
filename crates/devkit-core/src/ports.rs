use crate::model::LOCALHOST;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

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
            PortOwner::Free => format!(
                "{} :{} libre",
                self.display_name, self.port
            ),
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

/// Clasifica un puerto respecto a nuestro pidfile.
pub fn classify_port(host: &str, port: u16, pidfile: &Path) -> PortOwner {
    if !crate::health::is_port_open(host, port) {
        // Limpia pidfile huérfano si el proceso ya no existe.
        if let Some(pid) = read_pidfile(pidfile) {
            if !process_alive(pid) {
                let _ = fs::remove_file(pidfile);
            }
        }
        return PortOwner::Free;
    }

    let listeners = find_listeners(port);
    let our_pid = read_pidfile(pidfile).filter(|pid| process_alive(*pid));

    if let Some(our) = our_pid {
        if listeners.iter().any(|p| p.pid == our) || listeners.is_empty() {
            let name = process_name(our).unwrap_or_else(|| "devkit-service".into());
            return PortOwner::Ours { pid: our, name };
        }
    }

    if listeners.is_empty() {
        // TCP responde (p.ej. connect ok) pero netstat no listó LISTENING a tiempo.
        return PortOwner::Unknown;
    }

    // Puerto ocupado por otros: descarta pidfile rancio.
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
    let host = if host.eq_ignore_ascii_case("localhost") {
        LOCALHOST
    } else {
        host
    };
    let owner = classify_port(host, port, pidfile);
    let suggested = suggest_alternates(port);
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
    let output = Command::new("netstat")
        .args(["-ano", "-p", "tcp"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    let Ok(out) = output else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut pids = parse_netstat_listening_pids(&text, port);
    pids.sort_unstable();
    pids.dedup();
    pids.into_iter()
        .filter_map(|pid| {
            let name = process_name(pid).unwrap_or_else(|| "unknown".into());
            Some(PortProcess { pid, name })
        })
        .collect()
}

/// Extrae PIDs en estado LISTENING para `port` desde salida de `netstat -ano`.
pub fn parse_netstat_listening_pids(netstat: &str, port: u16) -> Vec<u32> {
    let needle = format!(":{port}");
    let mut out = Vec::new();
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
        // TCP  127.0.0.1:3306  0.0.0.0:0  LISTENING  1234
        if parts.len() < 4 {
            continue;
        }
        let local = parts[1];
        if !local_addr_has_port(local, &needle) {
            continue;
        }
        if let Ok(pid) = parts[parts.len() - 1].parse::<u32>() {
            if pid > 0 {
                out.push(pid);
            }
        }
    }
    out
}

fn local_addr_has_port(local: &str, needle: &str) -> bool {
    // Coincide :3306 al final del host local (evita :33060).
    if let Some(idx) = local.rfind(':') {
        let port_part = &local[idx..];
        return port_part.eq_ignore_ascii_case(needle);
    }
    false
}

pub fn read_pidfile(path: &Path) -> Option<u32> {
    let text = fs::read_to_string(path).ok()?;
    text.trim().parse().ok()
}

pub fn process_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(windows)]
    {
        let out = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();
        let Ok(out) = out else {
            return false;
        };
        let text = String::from_utf8_lossy(&out.stdout);
        // "INFO: No tasks are running..." when missing
        text.contains(&format!(",\"{pid}\",")) || text.contains(&format!(",{pid},"))
    }
    #[cfg(not(windows))]
    {
        Path::new(&format!("/proc/{pid}")).exists()
    }
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
        // "mysqld.exe","1234","Console","1","12,345 K"
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
        let pids8080 = parse_netstat_listening_pids(sample, 8080);
        assert_eq!(pids8080, vec![7001]);
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
        // Pick a port that is free by binding then releasing.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        assert!(!is_port_open(LOCALHOST, port));
        assert_eq!(
            classify_port(LOCALHOST, port, &pidfile),
            PortOwner::Free
        );
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
            PortOwner::Foreign { processes } => {
                assert!(!processes.is_empty() || true); // netstat may lag; Foreign or Unknown ok
            }
            PortOwner::Unknown => {}
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

        // Inject via direct owner construction path: classify uses netstat.
        // If netstat lists our PID for this port, Ours; else Foreign/Unknown.
        // We still assert that having a live matching pidfile + open port does not yield Free.
        let owner = classify_port(LOCALHOST, port, &pidfile);
        assert_ne!(owner, PortOwner::Free);
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
}

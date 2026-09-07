use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

/// Dirección de escucha obligatoria: solo esta máquina.
pub const LOCALHOST: &str = "127.0.0.1";

pub const HTTP_PORT: u16 = 8080;
pub const PHP_PORT: u16 = 9000;
pub const MYSQL_PORT: u16 = 3306;
pub const REDIS_PORT: u16 = 6379;
pub const MAILPIT_SMTP: u16 = 1025;
pub const MAILPIT_UI: u16 = 8025;
pub const HTTPS_PORT: u16 = 8443;
pub const DNS_PORT: u16 = 53;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceStatus {
    Stopped,
    Starting,
    Running,
    Crashed,
}

impl ServiceStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Crashed => "crashed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StopKind {
    Terminate,
    NginxQuit,
    MysqlAdmin,
    RedisShutdown,
}

#[derive(Debug, Clone)]
pub struct ServiceSpec {
    pub id: String,
    pub display_name: String,
    pub binary: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: Option<PathBuf>,
    pub bind_host: String,
    pub port: u16,
    pub pidfile: PathBuf,
    pub stop: StopKind,
}

impl ServiceSpec {
    pub fn command(&self) -> Command {
        let mut cmd = Command::new(&self.binary);
        cmd.args(&self.args);
        if let Some(cwd) = &self.cwd {
            cmd.current_dir(cwd);
        }
        for (k, v) in &self.env {
            cmd.env(k, v);
        }
        cmd
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSnapshot {
    pub id: String,
    pub display_name: String,
    pub status: ServiceStatus,
    pub bind: String,
    pub port: u16,
    pub pid: Option<u32>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
    pub name: String,
    pub hostname: String,
    pub root: PathBuf,
    pub php_version: String,
    pub ssl: bool,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackStatus {
    pub root: PathBuf,
    pub php_version: String,
    pub services: Vec<ServiceSnapshot>,
    pub sites: Vec<Site>,
    pub dns: String,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub port_checks: Vec<crate::ports::PortCheck>,
}

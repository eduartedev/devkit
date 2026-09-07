use crate::create;
use crate::dns;
use crate::error::{Error, Result};
use crate::ports::{check_service_port, PortCheck};
use crate::hosts;
use crate::model::{
    ServiceSnapshot, ServiceSpec, ServiceStatus, StackStatus, StopKind, LOCALHOST,
};
use crate::nginx;
use crate::pack;
use crate::paths::{AppConfig, Paths};
use crate::php::{write_mysql_ini, write_php_ini, write_redis_conf};
use crate::secrets::{mysql_password, redis_password};
use crate::setup::{list_php_versions, log_setup, run_setup};
use crate::site::scan_sites;
use crate::ssl;
use crate::supervisor::Supervisor;
use crate::terminal;
use std::fs;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const DETACHED: u32 = 0x0000_0008 | 0x0800_0000 | 0x0000_0200;

pub struct DevKit {
    pub paths: Paths,
    pub config: AppConfig,
    supervisor: Supervisor,
    dns_stop: Option<Arc<AtomicBool>>,
    dns_thread: Option<JoinHandle<()>>,
    pub last_dns: String,
}

impl DevKit {
    pub fn load() -> Result<Self> {
        let paths = Paths::detect()?;
        paths.ensure()?;
        let config = AppConfig::load(&paths.config_file())?;
        crate::bind::require_localhost(&config.bind)?;
        Ok(Self {
            paths,
            config,
            supervisor: Supervisor::new(),
            dns_stop: None,
            dns_thread: None,
            last_dns: "apagado".into(),
        })
    }

    pub fn setup(&mut self, full: bool, php: Option<&str>) -> Result<Vec<String>> {
        if let Some(v) = php {
            self.config.php_version = v.to_string();
            self.config.save(&self.paths.config_file())?;
        }
        let report = run_setup(&self.paths, &self.config, full, php)?;
        log_setup(&self.paths, &report)?;
        self.reload_sites()?;
        Ok(report.lines)
    }

    pub fn specs(&self) -> Result<Vec<ServiceSpec>> {
        let mut specs = Vec::new();
        specs.push(self.spec_nginx()?);
        specs.push(self.spec_php()?);
        specs.push(self.spec_mariadb()?);
        if self.paths.redis_server().exists() {
            specs.push(self.spec_redis()?);
        }
        if self.paths.mailpit().exists() {
            specs.push(self.spec_mailpit()?);
        }
        Ok(specs)
    }

    fn spec_nginx(&self) -> Result<ServiceSpec> {
        Ok(ServiceSpec {
            id: "nginx".into(),
            display_name: "Nginx".into(),
            binary: self.paths.nginx_exe(),
            args: vec![
                "-p".into(),
                self.paths.posix(&self.paths.root),
                "-c".into(),
                self.paths.posix(&self.paths.nginx_conf()),
            ],
            env: vec![],
            cwd: Some(self.paths.bin.join("nginx")),
            bind_host: self.config.bind.clone(),
            port: self.config.http_port,
            pidfile: self.paths.logs.join("nginx-svc.pid"),
            stop: StopKind::NginxQuit,
        })
    }

    fn spec_php(&self) -> Result<ServiceSpec> {
        let v = &self.config.php_version;
        Ok(ServiceSpec {
            id: "php".into(),
            display_name: format!("PHP {v} FastCGI"),
            binary: self.paths.php_cgi(v),
            args: vec!["-b".into(), format!("{}:{}", self.config.bind, self.config.php_port)],
            env: vec![
                ("PHP_FCGI_MAX_REQUESTS".into(), "0".into()),
                (
                    "PHPRC".into(),
                    self.paths.php_dir(v).to_string_lossy().into_owned(),
                ),
            ],
            cwd: Some(self.paths.php_dir(v)),
            bind_host: self.config.bind.clone(),
            port: self.config.php_port,
            pidfile: self.paths.logs.join("php-cgi.pid"),
            stop: StopKind::Terminate,
        })
    }

    fn spec_mariadb(&self) -> Result<ServiceSpec> {
        Ok(ServiceSpec {
            id: "mariadb".into(),
            display_name: "MariaDB".into(),
            binary: self.paths.mysqld(),
            args: vec![
                format!("--defaults-file={}", self.paths.posix(&self.paths.mysql_cnf())),
                "--console".into(),
            ],
            env: vec![],
            cwd: Some(self.paths.bin.join("mariadb")),
            bind_host: self.config.bind.clone(),
            port: self.config.mysql_port,
            pidfile: self.paths.logs.join("mariadb.pid"),
            stop: StopKind::MysqlAdmin,
        })
    }

    fn spec_redis(&self) -> Result<ServiceSpec> {
        Ok(ServiceSpec {
            id: "redis".into(),
            display_name: "Redis".into(),
            binary: self.paths.redis_server(),
            args: vec![self.paths.posix(&self.paths.redis_conf())],
            env: vec![],
            cwd: Some(self.paths.bin.join("redis")),
            bind_host: self.config.bind.clone(),
            port: self.config.redis_port,
            pidfile: self.paths.logs.join("redis.pid"),
            stop: StopKind::RedisShutdown,
        })
    }

    fn spec_mailpit(&self) -> Result<ServiceSpec> {
        Ok(ServiceSpec {
            id: "mailpit".into(),
            display_name: "Mailpit".into(),
            binary: self.paths.mailpit(),
            args: vec![
                "--listen".into(),
                format!("{}:{}", self.config.bind, self.config.mailpit_ui),
                "--smtp".into(),
                format!("{}:{}", self.config.bind, self.config.mailpit_smtp),
            ],
            env: vec![],
            cwd: Some(self.paths.bin.join("mailpit")),
            bind_host: self.config.bind.clone(),
            port: self.config.mailpit_ui,
            pidfile: self.paths.logs.join("mailpit.pid"),
            stop: StopKind::Terminate,
        })
    }

    pub fn reload_sites(&mut self) -> Result<Vec<crate::model::Site>> {
        write_mysql_ini(&self.paths, &self.config)?;
        write_php_ini(&self.paths.php_dir(&self.config.php_version), &self.config)?;
        if self.paths.redis_password_file().exists() || self.paths.redis_server().exists() {
            let pw = redis_password(&self.paths)?;
            write_redis_conf(&self.paths, &self.config, &pw)?;
        }
        let sites = scan_sites(&self.paths, &self.config)?;
        nginx::write_all(&self.paths, &self.config, &sites)?;
        if crate::health::is_port_open(&self.config.bind, self.config.http_port) {
            let spec = self.spec_nginx()?;
            let _ = Command::new(&spec.binary)
                .args([
                    "-p",
                    &self.paths.posix(&self.paths.root),
                    "-c",
                    &self.paths.posix(&self.paths.nginx_conf()),
                    "-s",
                    "reload",
                ])
                .current_dir(self.paths.bin.join("nginx"))
                .status();
        }
        Ok(sites)
    }

    pub fn start_dns_or_hosts(&mut self) -> Result<()> {
        if self.dns_stop.is_some() {
            return Ok(());
        }
        let sites = scan_sites(&self.paths, &self.config)?;
        match dns::try_bind() {
            Ok(_sock) => {
                drop(_sock);
                if let Ok(exe) = std::env::current_exe() {
                    if exe.file_name().and_then(|n| n.to_str()) == Some("devkit.exe") {
                        let pidfile = self.paths.logs.join("dns.pid");
                        let mut cmd = Command::new(&exe);
                        cmd.arg("dns-serve")
                            .stdin(Stdio::null())
                            .stdout(Stdio::null())
                            .stderr(Stdio::null());
                        #[cfg(windows)]
                        cmd.creation_flags(DETACHED);
                        if let Ok(child) = cmd.spawn() {
                            let _ = fs::write(&pidfile, child.id().to_string());
                            std::mem::forget(child);
                            self.last_dns =
                                format!("DNS local {LOCALHOST}:53 (*.{}) [detach]", self.config.tld);
                            let _ = hosts::apply_hosts(&sites);
                            return Ok(());
                        }
                    }
                }
                let stop = Arc::new(AtomicBool::new(false));
                let handle = dns::spawn(stop.clone())?;
                self.dns_stop = Some(stop);
                self.dns_thread = Some(handle);
                self.last_dns = format!("DNS local {LOCALHOST}:53 (*.{})", self.config.tld);
            }
            Err(e) => {
                match hosts::apply_hosts(&sites) {
                    Ok(()) => {
                        self.last_dns = "archivo hosts actualizado (fallback)".into();
                    }
                    Err(h) => {
                        self.last_dns = format!("{e} | {h}");
                    }
                }
            }
        }
        Ok(())
    }

    pub fn stop_dns(&mut self) {
        if let Some(s) = &self.dns_stop {
            s.store(true, Ordering::Relaxed);
        }
        self.dns_stop = None;
        self.dns_thread = None;
        let pidfile = self.paths.logs.join("dns.pid");
        if let Ok(text) = fs::read_to_string(&pidfile) {
            if let Ok(pid) = text.trim().parse::<u32>() {
                let _ = Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/F", "/T"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            let _ = fs::remove_file(pidfile);
        }
        self.last_dns = "apagado".into();
    }

    fn init_mariadb_if_needed(&self) -> Result<()> {
        write_mysql_ini(&self.paths, &self.config)?;
        let datadir = self.paths.data.join("mysql");
        fs::create_dir_all(&datadir)?;
        let marker = datadir.join("mysql");
        if marker.exists() {
            return Ok(());
        }
        let installer = self.paths.mysql_install_db();
        if installer.exists() {
            let status = Command::new(&installer)
                .arg(format!("--datadir={}", self.paths.posix(&datadir)))
                .status()?;
            if !status.success() {
                return Err(Error::msg("mysql_install_db falló"));
            }
            return Ok(());
        }
        let mysqld = self.paths.mysqld();
        if mysqld.exists() {
            let status = Command::new(&mysqld)
                .args([
                    format!("--datadir={}", self.paths.posix(&datadir)),
                    "--initialize-insecure".into(),
                ])
                .status()?;
            if !status.success() {
                return Err(Error::msg("mysqld --initialize-insecure falló"));
            }
        }
        Ok(())
    }

    fn set_mysql_password_if_needed(&self) -> Result<()> {
        let password = mysql_password(&self.paths)?;
        let admin = self.paths.mysqladmin();
        if !admin.exists() {
            return Ok(());
        }
        let _ = Command::new(&admin)
            .args([
                "-u",
                "root",
                "-h",
                "127.0.0.1",
                "-P",
                &self.config.mysql_port.to_string(),
                "password",
                &password,
            ])
            .status();
        Ok(())
    }

    pub fn check_ports(&self) -> Result<Vec<PortCheck>> {
        let mut checks = Vec::new();
        for spec in self.specs()? {
            checks.push(check_service_port(
                &spec.id,
                &spec.display_name,
                &spec.bind_host,
                spec.port,
                &spec.pidfile,
            ));
        }
        Ok(checks)
    }

    pub fn start_all(&mut self, detach: bool) -> Result<StackStatus> {
        self.paths.ensure()?;
        self.reload_sites()?;
        self.start_dns_or_hosts()?;
        let mut warnings = Vec::new();

        let port_checks = self.check_ports().unwrap_or_default();
        for check in &port_checks {
            if check.is_conflict() {
                warnings.push(check.summary());
            }
        }

        if self.paths.mysqld().exists() {
            if let Err(e) = self.init_mariadb_if_needed() {
                warnings.push(format!("MariaDB init: {e}"));
            }
        }

        let order = ["mariadb", "php", "nginx", "redis", "mailpit"];
        let specs = self.specs()?;
        for id in order {
            if let Some(spec) = specs.iter().find(|s| s.id == id).cloned() {
                // Skip spawn if already conflicted — avoid noisy double errors.
                let blocked = port_checks
                    .iter()
                    .find(|c| c.service == id)
                    .is_some_and(|c| c.is_conflict());
                if blocked {
                    continue;
                }
                match self.supervisor.start(spec, detach) {
                    Ok(snap) => {
                        if snap.status == ServiceStatus::Crashed {
                            warnings.push(format!("{}: {}", snap.display_name, snap.detail));
                        }
                    }
                    Err(e) => warnings.push(e.to_string()),
                }
            }
        }
        if crate::health::is_port_open(&self.config.bind, self.config.mysql_port) {
            let _ = self.set_mysql_password_if_needed();
        }
        let mut st = self.status()?;
        st.warnings.extend(warnings);
        st.port_checks = self.check_ports().unwrap_or_default();
        Ok(st)
    }

    pub fn stop_all(&mut self) -> Result<StackStatus> {
        let specs = self.specs().unwrap_or_default();
        for spec in specs.iter().rev() {
            if spec.id == "mariadb" {
                let pw = mysql_password(&self.paths).unwrap_or_default();
                let _ = self
                    .supervisor
                    .stop_mysql_admin(spec, &self.paths.mysqladmin(), &pw);
            } else {
                let _ = self.supervisor.stop(spec);
            }
        }
        self.stop_dns();
        self.status()
    }

    pub fn start_service(&mut self, id: &str, detach: bool) -> Result<ServiceSnapshot> {
        self.reload_sites()?;
        if id == "mariadb" {
            self.init_mariadb_if_needed()?;
        }
        let spec = self
            .specs()?
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| Error::msg(format!("servicio desconocido: {id}")))?;
        let snap = self.supervisor.start(spec, detach)?;
        if id == "mariadb" && snap.status == ServiceStatus::Running {
            let _ = self.set_mysql_password_if_needed();
        }
        Ok(snap)
    }

    pub fn stop_service(&mut self, id: &str) -> Result<ServiceSnapshot> {
        let spec = self
            .specs()?
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| Error::msg(format!("servicio desconocido: {id}")))?;
        if spec.id == "mariadb" {
            let pw = mysql_password(&self.paths).unwrap_or_default();
            return self
                .supervisor
                .stop_mysql_admin(&spec, &self.paths.mysqladmin(), &pw);
        }
        self.supervisor.stop(&spec)
    }

    pub fn status(&self) -> Result<StackStatus> {
        let specs = self.specs().unwrap_or_default();
        let services: Vec<_> = specs
            .iter()
            .map(|s| self.supervisor.snapshot(s))
            .collect();
        let sites = scan_sites(&self.paths, &self.config)?;
        let port_checks = self.check_ports().unwrap_or_default();
        let mut warnings = Vec::new();
        for check in &port_checks {
            if check.is_conflict() {
                warnings.push(check.summary());
            }
        }
        Ok(StackStatus {
            root: self.paths.root.clone(),
            php_version: self.config.php_version.clone(),
            services,
            sites,
            dns: self.last_dns.clone(),
            warnings,
            port_checks,
        })
    }

    pub fn use_php(&mut self, version: &str) -> Result<()> {
        let cgi = self.paths.php_cgi(version);
        if !cgi.exists() {
            return Err(Error::msg(format!(
                "PHP {version} no está en {}. Ejecuta `devkit setup --php {version}`",
                cgi.display()
            )));
        }
        let was_running = matches!(
            check_service_port(
                "php",
                "PHP",
                &self.config.bind,
                self.config.php_port,
                &self.paths.logs.join("php-cgi.pid"),
            )
            .owner,
            crate::ports::PortOwner::Ours { .. }
        );
        if was_running {
            let _ = self.stop_service("php");
        }
        self.config.php_version = version.to_string();
        self.config.save(&self.paths.config_file())?;
        write_php_ini(&self.paths.php_dir(version), &self.config)?;
        self.reload_sites()?;
        if was_running {
            self.start_service("php", true)?;
        }
        Ok(())
    }

    pub fn php_versions(&self) -> Result<Vec<String>> {
        list_php_versions(&self.paths)
    }

    pub fn create_site(&mut self, kind: &str, name: &str) -> Result<std::path::PathBuf> {
        let path = match kind {
            "blank" | "php" => create::create_blank(&self.paths, name)?,
            "laravel" => {
                create::create_laravel(&self.paths, &self.config.php_version, name)?
            }
            "wordpress" | "wp" => create::create_wordpress(&self.paths, name)?,
            other => {
                return Err(Error::msg(format!(
                    "tipo desconocido '{other}' (blank|laravel|wordpress)"
                )))
            }
        };
        self.reload_sites()?;
        Ok(path)
    }

    pub fn enable_ssl(&mut self) -> Result<String> {
        let sites = scan_sites(&self.paths, &self.config)?;
        let msg = ssl::issue_certs(&self.paths, &mut self.config, &sites)?;
        self.reload_sites()?;
        Ok(msg)
    }

    pub fn pack(&self, out: Option<std::path::PathBuf>) -> Result<std::path::PathBuf> {
        pack::pack(&self.paths, out)
    }

    pub fn open_terminal(&self) -> Result<()> {
        terminal::open_terminal(&self.paths, &self.config)
    }

    pub fn isolated_path(&self) -> String {
        terminal::isolated_path(&self.paths, &self.config)
    }

    pub fn adminer_url(&self) -> String {
        format!("http://{}:{}/adminer/", self.config.bind, self.config.http_port)
    }

    pub fn default_url(&self) -> String {
        format!("http://{}:{}/", self.config.bind, self.config.http_port)
    }
}

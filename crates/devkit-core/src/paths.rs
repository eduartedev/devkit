use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
    pub bin: PathBuf,
    pub etc: PathBuf,
    pub data: PathBuf,
    pub www: PathBuf,
    pub logs: PathBuf,
    pub tmp: PathBuf,
}

impl Paths {
    pub fn detect() -> Result<Self> {
        if let Ok(env) = std::env::var("DEVKIT_ROOT") {
            return Ok(Self::from_root(PathBuf::from(env)));
        }
        if let Ok(exe) = std::env::current_exe() {
            for ancestor in exe.ancestors() {
                if ancestor.join(".devkit-root").is_file() {
                    return Ok(Self::from_root(ancestor.to_path_buf()));
                }
            }
        }
        let fallback = PathBuf::from(r"C:\DevKit");
        if fallback.join(".devkit-root").is_file() || fallback.is_dir() {
            return Ok(Self::from_root(fallback));
        }
        Ok(Self::from_root(std::env::current_dir()?))
    }

    pub fn from_root(root: PathBuf) -> Self {
        Self {
            bin: root.join("bin"),
            etc: root.join("etc"),
            data: root.join("data"),
            www: root.join("www"),
            logs: root.join("logs"),
            tmp: root.join("tmp"),
            root,
        }
    }

    pub fn ensure(&self) -> Result<()> {
        for dir in [
            &self.root,
            &self.bin,
            &self.etc,
            &self.data,
            &self.www,
            &self.logs,
            &self.tmp,
            &self.etc.join("devkit"),
            &self.etc.join("devkit").join("secrets"),
            &self.etc.join("nginx").join("sites"),
            &self.etc.join("mysql"),
            &self.etc.join("redis"),
            &self.etc.join("ssl"),
            &self.data.join("mysql"),
            &self.data.join("redis"),
            &self.www.join("default"),
            &self.bin.join("php"),
            &self.bin.join("nginx"),
            &self.bin.join("mariadb"),
            &self.bin.join("redis"),
            &self.bin.join("mailpit"),
            &self.bin.join("mkcert"),
            &self.bin.join("node"),
            &self.bin.join("composer"),
        ] {
            fs::create_dir_all(dir)?;
        }
        let marker = self.root.join(".devkit-root");
        if !marker.exists() {
            fs::write(marker, "devkit\n")?;
        }
        Ok(())
    }

    pub fn config_file(&self) -> PathBuf {
        self.etc.join("devkit").join("config.toml")
    }

    pub fn nginx_conf(&self) -> PathBuf {
        self.etc.join("nginx").join("nginx.conf")
    }

    pub fn nginx_sites(&self) -> PathBuf {
        self.etc.join("nginx").join("sites")
    }

    pub fn mysql_cnf(&self) -> PathBuf {
        self.etc.join("mysql").join("my.ini")
    }

    pub fn redis_conf(&self) -> PathBuf {
        self.etc.join("redis").join("redis.conf")
    }

    pub fn php_dir(&self, version: &str) -> PathBuf {
        self.bin.join("php").join(version)
    }

    pub fn php_cgi(&self, version: &str) -> PathBuf {
        self.php_dir(version).join("php-cgi.exe")
    }

    pub fn php_exe(&self, version: &str) -> PathBuf {
        self.php_dir(version).join("php.exe")
    }

    pub fn nginx_exe(&self) -> PathBuf {
        self.bin.join("nginx").join("nginx.exe")
    }

    pub fn mysqld(&self) -> PathBuf {
        let a = self.bin.join("mariadb").join("bin").join("mysqld.exe");
        if a.exists() {
            a
        } else {
            self.bin.join("mariadb").join("mysqld.exe")
        }
    }

    pub fn mysqladmin(&self) -> PathBuf {
        let a = self.bin.join("mariadb").join("bin").join("mysqladmin.exe");
        if a.exists() {
            a
        } else {
            self.bin.join("mariadb").join("mysqladmin.exe")
        }
    }

    pub fn mysql_install_db(&self) -> PathBuf {
        for name in ["mariadb-install-db.exe", "mysql_install_db.exe"] {
            let p = self.bin.join("mariadb").join("bin").join(name);
            if p.exists() {
                return p;
            }
            let p2 = self.bin.join("mariadb").join(name);
            if p2.exists() {
                return p2;
            }
        }
        self.bin.join("mariadb").join("bin").join("mysql_install_db.exe")
    }

    pub fn redis_server(&self) -> PathBuf {
        self.bin.join("redis").join("redis-server.exe")
    }

    pub fn mailpit(&self) -> PathBuf {
        self.bin.join("mailpit").join("mailpit.exe")
    }

    pub fn mkcert(&self) -> PathBuf {
        self.bin.join("mkcert").join("mkcert.exe")
    }

    pub fn node_exe(&self) -> PathBuf {
        let nested = self.bin.join("node").join("node.exe");
        if nested.exists() {
            nested
        } else {
            // node zip is node-vXX-win-x64/node.exe
            nested
        }
    }

    pub fn composer(&self) -> PathBuf {
        self.bin.join("composer").join("composer.phar")
    }

    pub fn secrets_dir(&self) -> PathBuf {
        self.etc.join("devkit").join("secrets")
    }

    pub fn mysql_password_file(&self) -> PathBuf {
        self.secrets_dir().join("mysql-root.txt")
    }

    pub fn mysql_user_file(&self) -> PathBuf {
        self.secrets_dir().join("mysql-user.txt")
    }

    pub fn redis_password_file(&self) -> PathBuf {
        self.secrets_dir().join("redis.txt")
    }

    pub fn ssl_cert(&self) -> PathBuf {
        self.etc.join("ssl").join("devkit.pem")
    }

    pub fn ssl_key(&self) -> PathBuf {
        self.etc.join("ssl").join("devkit-key.pem")
    }

    pub fn posix(&self, path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub bind: String,
    pub http_port: u16,
    pub https_port: u16,
    pub php_port: u16,
    pub mysql_port: u16,
    pub redis_port: u16,
    pub mailpit_smtp: u16,
    pub mailpit_ui: u16,
    pub php_version: String,
    pub ssl: bool,
    pub tld: String,
    /// Extensiones PHP activas (nombres sin `php_` / `.dll`). Vacío = defaults.
    #[serde(default = "default_php_extensions")]
    pub php_extensions: Vec<String>,
    /// Si true, carpetas en www/ que sean junction/symlink cuentan como sitios.
    #[serde(default = "default_true")]
    pub follow_www_junctions: bool,
    /// Segundos de `fastcgi_read_timeout` y `fastcgi_send_timeout` en cada `location ~ \.php$`.
    #[serde(default = "default_fastcgi_timeout")]
    pub fastcgi_timeout_secs: u64,
    #[serde(default)]
    pub launch_at_login: bool,
    #[serde(default)]
    pub start_hidden: bool,
    #[serde(default)]
    pub auto_start_mode: AutoStartMode,
    #[serde(default)]
    pub auto_start_services: Vec<String>,
}

/// IDs de servicios que se pueden auto-arrancar al abrir el panel.
pub const AUTO_START_SERVICE_IDS: [&str; 5] =
    ["mariadb", "php", "nginx", "redis", "mailpit"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AutoStartMode {
    #[default]
    Off,
    All,
    Selected,
}

fn default_true() -> bool {
    true
}

pub const FASTCGI_TIMEOUT_MIN: u64 = 1;
pub const FASTCGI_TIMEOUT_MAX: u64 = 7200;

fn default_fastcgi_timeout() -> u64 {
    60
}

fn default_php_extensions() -> Vec<String> {
    [
        "mysqli",
        "pdo_mysql",
        "pdo_sqlite",
        "sqlite3",
        "openssl",
        "mbstring",
        "curl",
        "fileinfo",
        "zip",
        "gd",
        "intl",
        "exif",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            bind: crate::model::LOCALHOST.to_string(),
            http_port: crate::model::HTTP_PORT,
            https_port: crate::model::HTTPS_PORT,
            php_port: crate::model::PHP_PORT,
            mysql_port: crate::model::MYSQL_PORT,
            redis_port: crate::model::REDIS_PORT,
            mailpit_smtp: crate::model::MAILPIT_SMTP,
            mailpit_ui: crate::model::MAILPIT_UI,
            php_version: "8.3".to_string(),
            ssl: false,
            tld: "test".to_string(),
            php_extensions: default_php_extensions(),
            follow_www_junctions: true,
            fastcgi_timeout_secs: default_fastcgi_timeout(),
            launch_at_login: false,
            start_hidden: false,
            auto_start_mode: AutoStartMode::Off,
            auto_start_services: Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            let cfg = Self::default();
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, toml::to_string_pretty(&cfg)?)?;
            return Ok(cfg);
        }
        let text = fs::read_to_string(path)?;
        Ok(toml::from_str(&text)?)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        crate::bind::require_localhost(&self.bind)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}

/// Ajustes de la app DevKit (no puertos ni secretos de DB).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    pub tld: String,
    pub follow_www_junctions: bool,
    pub fastcgi_timeout_secs: u64,
    pub launch_at_login: bool,
    pub start_hidden: bool,
    pub auto_start_mode: AutoStartMode,
    pub auto_start_services: Vec<String>,
}

/// Filtra y ordena IDs de auto-arranque (orden del stack).
pub fn ordered_auto_start_ids(requested: &[String]) -> Vec<String> {
    AUTO_START_SERVICE_IDS
        .iter()
        .filter(|id| requested.iter().any(|r| r == *id))
        .map(|id| id.to_string())
        .collect()
}

impl AppSettings {
    pub fn from_config(cfg: &AppConfig) -> Self {
        Self {
            tld: cfg.tld.clone(),
            follow_www_junctions: cfg.follow_www_junctions,
            fastcgi_timeout_secs: cfg.fastcgi_timeout_secs,
            launch_at_login: cfg.launch_at_login,
            start_hidden: cfg.start_hidden,
            auto_start_mode: cfg.auto_start_mode.clone(),
            auto_start_services: cfg.auto_start_services.clone(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        let tld = self.tld.trim().to_ascii_lowercase();
        if tld.is_empty() {
            return Err(Error::msg("TLD vacío"));
        }
        if tld.len() > 24 {
            return Err(Error::msg("TLD demasiado largo"));
        }
        if !tld
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return Err(Error::msg(
                "TLD inválido: solo a-z, 0-9 y guion (ej. test)",
            ));
        }
        if tld.starts_with('-') || tld.ends_with('-') {
            return Err(Error::msg("TLD no puede empezar o terminar con guion"));
        }
        if !(FASTCGI_TIMEOUT_MIN..=FASTCGI_TIMEOUT_MAX).contains(&self.fastcgi_timeout_secs) {
            return Err(Error::msg(format!(
                "tiempo de espera PHP debe estar entre {FASTCGI_TIMEOUT_MIN} y {FASTCGI_TIMEOUT_MAX} segundos"
            )));
        }
        for id in &self.auto_start_services {
            if !AUTO_START_SERVICE_IDS.contains(&id.as_str()) {
                return Err(Error::msg(format!("servicio de auto-arranque inválido: {id}")));
            }
        }
        Ok(())
    }

    pub fn apply_to(&self, cfg: &mut AppConfig) {
        cfg.tld = self.tld.trim().to_ascii_lowercase();
        cfg.follow_www_junctions = self.follow_www_junctions;
        cfg.fastcgi_timeout_secs = self.fastcgi_timeout_secs;
        cfg.launch_at_login = self.launch_at_login;
        cfg.start_hidden = self.start_hidden;
        cfg.auto_start_mode = self.auto_start_mode.clone();
        cfg.auto_start_services = ordered_auto_start_ids(&self.auto_start_services);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_config_without_timeout_stays_at_60() {
        let raw = toml::to_string(&AppConfig::default()).unwrap();
        let stripped = raw
            .lines()
            .filter(|line| !line.contains("fastcgi_timeout_secs"))
            .collect::<Vec<_>>()
            .join("\n");
        let cfg: AppConfig = toml::from_str(&stripped).unwrap();
        assert_eq!(cfg.fastcgi_timeout_secs, 60);
    }

    #[test]
    fn timeout_rejects_zero_and_over_max() {
        let mut settings = AppSettings::from_config(&AppConfig::default());
        settings.fastcgi_timeout_secs = 0;
        assert!(settings.validate().is_err());
        settings.fastcgi_timeout_secs = FASTCGI_TIMEOUT_MAX + 1;
        assert!(settings.validate().is_err());
        settings.fastcgi_timeout_secs = 3600;
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn old_config_without_startup_fields_uses_defaults() {
        let raw = toml::to_string(&AppConfig::default()).unwrap();
        let stripped = raw
            .lines()
            .filter(|line| {
                !line.contains("launch_at_login")
                    && !line.contains("start_hidden")
                    && !line.contains("auto_start")
            })
            .collect::<Vec<_>>()
            .join("\n");
        let cfg: AppConfig = toml::from_str(&stripped).unwrap();
        assert!(!cfg.launch_at_login);
        assert!(!cfg.start_hidden);
        assert_eq!(cfg.auto_start_mode, AutoStartMode::Off);
        assert!(cfg.auto_start_services.is_empty());
    }

    #[test]
    fn ordered_auto_start_ids_filters_and_sorts() {
        let ids = ordered_auto_start_ids(&[
            "mailpit".into(),
            "bogus".into(),
            "nginx".into(),
            "mariadb".into(),
        ]);
        assert_eq!(
            ids,
            ["mariadb", "nginx", "mailpit"]
                .map(String::from)
                .to_vec()
        );
    }

    #[test]
    fn validate_rejects_unknown_auto_start_service() {
        let mut settings = AppSettings::from_config(&AppConfig::default());
        settings.auto_start_services = vec!["docker".into()];
        assert!(settings.validate().is_err());
    }

    #[test]
    fn startup_settings_roundtrip_in_toml_and_apply() {
        let mut cfg = AppConfig::default();
        cfg.launch_at_login = true;
        cfg.start_hidden = true;
        cfg.auto_start_mode = AutoStartMode::Selected;
        cfg.auto_start_services = vec!["nginx".into(), "php".into(), "mariadb".into()];
        let text = toml::to_string_pretty(&cfg).unwrap();
        let loaded: AppConfig = toml::from_str(&text).unwrap();
        assert!(loaded.launch_at_login);
        assert!(loaded.start_hidden);
        assert_eq!(loaded.auto_start_mode, AutoStartMode::Selected);

        let settings = AppSettings::from_config(&loaded);
        assert!(settings.validate().is_ok());
        let mut out = AppConfig::default();
        settings.apply_to(&mut out);
        assert_eq!(
            out.auto_start_services,
            ["mariadb", "php", "nginx"].map(String::from).to_vec()
        );
    }
}

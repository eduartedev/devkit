use crate::error::Result;
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

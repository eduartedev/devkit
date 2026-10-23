use crate::error::{Error, Result};
use crate::paths::{AppConfig, Paths};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_PHP_EXTENSIONS: &[&str] = &[
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
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhpExtensionInfo {
    /// Nombre lógico (sin php_ ni .dll), p.ej. `gd`.
    pub name: String,
    /// Nombre del archivo en ext/, p.ej. `php_gd.dll`.
    pub dll: String,
    pub enabled: bool,
}

pub fn default_php_extensions() -> Vec<String> {
    DEFAULT_PHP_EXTENSIONS
        .iter()
        .map(|s| (*s).to_string())
        .collect()
}

/// Extrae el nombre de extensión desde `php_gd.dll` / `gd.dll` → `gd`.
pub fn extension_name_from_dll(file_name: &str) -> Option<String> {
    let lower = file_name.to_ascii_lowercase();
    if !lower.ends_with(".dll") {
        return None;
    }
    let stem = &lower[..lower.len() - 4];
    let name = stem.strip_prefix("php_").unwrap_or(stem);
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some(name.to_string())
}

pub fn scan_ext_dlls(php_dir: &Path) -> Result<Vec<(String, String)>> {
    let ext_dir = php_dir.join("ext");
    if !ext_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(&ext_dir)? {
        let entry = entry?;
        let fname = entry.file_name();
        let Some(name) = fname.to_str() else {
            continue;
        };
        if let Some(ext_name) = extension_name_from_dll(name) {
            out.push((ext_name, name.to_string()));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.dedup_by(|a, b| a.0 == b.0);
    Ok(out)
}

pub fn parse_enabled_extensions(ini: &str) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    for line in ini.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with(';') || t.starts_with('#') {
            continue;
        }
        let lower = t.to_ascii_lowercase();
        let Some(rest) = lower
            .strip_prefix("extension")
            .and_then(|s| s.trim_start().strip_prefix('='))
            .map(|s| s.trim())
        else {
            continue;
        };
        let rest = rest.trim_matches('"').trim_matches('\'');
        let file = Path::new(rest)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(rest);
        if let Some(name) = extension_name_from_dll(file) {
            set.insert(name);
        } else if !rest.is_empty()
            && rest
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            set.insert(rest.strip_prefix("php_").unwrap_or(rest).to_string());
        }
    }
    set
}

pub fn strip_devkit_block(ini: &str) -> String {
    const BEGIN: &str = "; DevKit";
    if let Some(start) = ini.find(BEGIN) {
        // Bloque hasta el próximo `[section]` al inicio de línea, o fin.
        let after = &ini[start + BEGIN.len()..];
        let mut end_rel = after.len();
        for (i, line) in after.lines().enumerate() {
            if i == 0 {
                continue;
            }
            let t = line.trim();
            if t.starts_with('[') && t.ends_with(']') {
                end_rel = after.find(line).unwrap_or(end_rel);
                break;
            }
        }
        let mut out = String::new();
        out.push_str(ini[..start].trim_end());
        let rest = after.get(end_rel..).unwrap_or("").trim_start_matches(['\r', '\n']);
        if !rest.is_empty() {
            out.push_str("\r\n\r\n");
            out.push_str(rest);
        }
        if !out.ends_with('\n') {
            out.push_str("\r\n");
        }
        return out;
    }
    ini.to_string()
}

pub fn build_devkit_php_block(
    php_dir: &Path,
    cfg: &AppConfig,
    extensions: &[String],
    error_log_posix: &str,
) -> String {
    let ext = php_dir.join("ext");
    let ext_posix = ext.to_string_lossy().replace('\\', "/");
    let mut lines = vec![
        "; DevKit".into(),
        format!("extension_dir = \"{ext_posix}\""),
        "cgi.force_redirect = 0".into(),
        "cgi.fix_pathinfo = 1".into(),
        "fastcgi.impersonate = 1".into(),
        "fastcgi.logging = 0".into(),
        format!("SMTP = {}", cfg.bind),
        format!("smtp_port = {}", cfg.mailpit_smtp),
        "date.timezone = UTC".into(),
        "upload_max_filesize = 64M".into(),
        "post_max_size = 64M".into(),
        "memory_limit = 256M".into(),
        // Mitigación php2shell / CVE-2026-109619 hasta parche aplicado (recomendación PHP).
        "session.upload_progress.enabled = 0".into(),
        "display_errors = On".into(),
        "log_errors = On".into(),
        "error_reporting = E_ALL".into(),
        format!("error_log = \"{error_log_posix}\""),
    ];
    let mut seen = BTreeSet::new();
    for name in extensions {
        let n = name.trim().to_ascii_lowercase();
        if n.is_empty() || !seen.insert(n.clone()) {
            continue;
        }
        if !n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        lines.push(format!("extension={n}"));
    }
    format!("\r\n{}\r\n", lines.join("\r\n"))
}

pub fn write_php_ini(paths: &Paths, php_dir: &Path, cfg: &AppConfig) -> Result<()> {
    fs::create_dir_all(&paths.logs)?;
    let dest = php_dir.join("php.ini");
    let src = if php_dir.join("php.ini-development").exists() {
        php_dir.join("php.ini-development")
    } else {
        php_dir.join("php.ini-production")
    };
    let base = if dest.exists() {
        fs::read_to_string(&dest)?
    } else if src.exists() {
        fs::read_to_string(&src)?
    } else {
        String::new()
    };
    let cleaned = strip_devkit_block(&base);
    let exts = if cfg.php_extensions.is_empty() {
        default_php_extensions()
    } else {
        cfg.php_extensions.clone()
    };
    let error_log = paths.posix(&paths.logs.join("php_error.log"));
    let mut body = cleaned.trim_end().to_string();
    body.push_str(&build_devkit_php_block(php_dir, cfg, &exts, &error_log));
    fs::write(dest, body)?;
    Ok(())
}

pub fn list_php_extensions(php_dir: &Path, cfg: &AppConfig) -> Result<Vec<PhpExtensionInfo>> {
    let dlls = scan_ext_dlls(php_dir)?;
    let _ini_path = php_dir.join("php.ini");
    let _enabled_ini = BTreeSet::<String>::new();
    let configured: BTreeSet<String> = if cfg.php_extensions.is_empty() {
        default_php_extensions().into_iter().collect()
    } else {
        cfg.php_extensions.iter().map(|s| s.to_ascii_lowercase()).collect()
    };
    Ok(dlls
        .into_iter()
        .map(|(name, dll)| PhpExtensionInfo {
            enabled: configured.contains(&name),
            name,
            dll,
        })
        .collect())
}

pub fn validate_extension_names(names: &[String], php_dir: &Path) -> Result<Vec<String>> {
    let available: BTreeSet<String> = scan_ext_dlls(php_dir)?
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for name in names {
        let n = name.trim().to_ascii_lowercase();
        if n.is_empty() {
            continue;
        }
        if !n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(Error::msg(format!("extensión inválida: {name}")));
        }
        if !available.is_empty() && !available.contains(&n) {
            return Err(Error::msg(format!(
                "no hay php_{n}.dll (ni {n}.dll) en ext/"
            )));
        }
        if seen.insert(n.clone()) {
            out.push(n);
        }
    }
    Ok(out)
}

pub fn write_mysql_ini(paths: &Paths, cfg: &AppConfig) -> Result<()> {
    fs::create_dir_all(&paths.logs)?;
    let datadir = paths.posix(&paths.data.join("mysql"));
    let basedir = paths.posix(&paths.bin.join("mariadb"));
    let error_log = paths.posix(&paths.logs.join("mariadb-error.log"));
    let creds = crate::secrets::mysql_credentials(paths)?;
    let password_line = if creds.password.is_empty() {
        String::new()
    } else {
        format!("password={}\n", creds.password)
    };
    let body = format!(
        r#"[mysqld]
basedir={basedir}
datadir={datadir}
port={port}
bind-address={bind}
skip-networking=0
log-error={error_log}
innodb_buffer_pool_size=128M
character-set-server=utf8mb4
collation-server=utf8mb4_unicode_ci
max_allowed_packet=64M

[client]
host={bind}
port={port}
user={user}
{password_line}"#,
        bind = cfg.bind,
        port = cfg.mysql_port,
        user = creds.user,
        password_line = password_line,
        error_log = error_log,
    );
    crate::bind::assert_config_localhost_only(&body)?;
    fs::write(paths.mysql_cnf(), body)?;
    Ok(())
}

pub fn write_redis_conf(paths: &Paths, cfg: &AppConfig, password: &str) -> Result<()> {
    fs::create_dir_all(&paths.logs)?;
    let dir = paths.posix(&paths.data.join("redis"));
    let logfile = paths.posix(&paths.logs.join("redis.log"));
    let body = format!(
        r#"bind {bind}
protected-mode yes
port {port}
requirepass {password}
dir {dir}
dbfilename dump.rdb
logfile {logfile}
loglevel notice
save ""
rename-command FLUSHALL ""
rename-command FLUSHDB ""
rename-command CONFIG ""
"#,
        bind = cfg.bind,
        port = cfg.redis_port,
        password = password,
        logfile = logfile,
    );
    crate::bind::assert_config_localhost_only(&body)?;
    fs::write(paths.redis_conf(), body)?;
    Ok(())
}

pub fn write_default_index(paths: &Paths) -> Result<()> {
    let index = paths.www.join("default").join("index.php");
    if index.exists() {
        return Ok(());
    }
    fs::write(
        index,
        r#"<?php
declare(strict_types=1);
header('X-Powered-By: DevKit');
?>
<!DOCTYPE html>
<html lang="es">
<head>
  <meta charset="utf-8">
  <title>DevKit</title>
  <style>
    body { font-family: ui-monospace, monospace; background:#12151a; color:#e8e4dc; margin:48px; }
    a { color:#3ee0a0; }
    .ok { color:#3ee0a0; }
  </style>
</head>
<body>
  <h1>DevKit</h1>
  <p class="ok">PHP <?php echo htmlspecialchars(PHP_VERSION, ENT_QUOTES, 'UTF-8'); ?> responde en 127.0.0.1</p>
  <p><a href="/">Inicio</a></p>
</body>
</html>
"#,
    )?;
    Ok(())
}

pub fn enable_php_extensions_safe(paths: &Paths, php_dir: &Path) -> Result<()> {
    write_php_ini(paths, php_dir, &AppConfig::default())
}

pub fn php_ini_path(php_dir: &Path) -> PathBuf {
    php_dir.join("php.ini")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn dll_name_parsing() {
        assert_eq!(extension_name_from_dll("php_gd.dll").as_deref(), Some("gd"));
        assert_eq!(extension_name_from_dll("pdo_mysql.dll").as_deref(), Some("pdo_mysql"));
        assert!(extension_name_from_dll("readme.txt").is_none());
    }

    #[test]
    fn parse_enabled_from_ini() {
        let ini = "extension=gd\r\nextension = php_curl.dll\r\n;extension=zip\r\n";
        let set = parse_enabled_extensions(ini);
        assert!(set.contains("gd"));
        assert!(set.contains("curl"));
        assert!(!set.contains("zip"));
    }

    #[test]
    fn strip_and_rebuild_devkit_block() {
        let dir = tempdir().unwrap();
        let php_dir = dir.path().join("php");
        fs::create_dir_all(php_dir.join("ext")).unwrap();
        fs::write(php_dir.join("ext").join("php_gd.dll"), []).unwrap();
        let base = "memory_limit=128M\r\n; DevKit\r\nextension=old\r\n[PHP]\r\nfoo=1\r\n";
        let cleaned = strip_devkit_block(base);
        assert!(!cleaned.contains("extension=old"));
        assert!(cleaned.contains("[PHP]"));
        let cfg = AppConfig::default();
        let block = build_devkit_php_block(
            &php_dir,
            &cfg,
            &["gd".into()],
            "C:/DevKit/logs/php_error.log",
        );
        assert!(block.contains("extension=gd"));
        assert!(block.contains("; DevKit"));
        assert!(block.contains("error_log = \"C:/DevKit/logs/php_error.log\""));
        assert!(block.contains("log_errors = On"));
        assert!(block.contains("session.upload_progress.enabled = 0"));
    }

    #[test]
    fn mysql_and_redis_log_under_logs_dir() {
        let dir = tempdir().unwrap();
        let paths = Paths::from_root(dir.path().to_path_buf());
        fs::create_dir_all(paths.secrets_dir()).unwrap();
        fs::create_dir_all(paths.etc.join("mysql")).unwrap();
        fs::create_dir_all(paths.etc.join("redis")).unwrap();
        let cfg = AppConfig::default();
        write_mysql_ini(&paths, &cfg).unwrap();
        let my = fs::read_to_string(paths.mysql_cnf()).unwrap();
        assert!(my.contains("log-error="));
        assert!(my.contains("mariadb-error.log"));
        write_redis_conf(&paths, &cfg, "secret").unwrap();
        let rd = fs::read_to_string(paths.redis_conf()).unwrap();
        assert!(rd.contains("logfile "));
        assert!(rd.contains("redis.log"));
    }
}

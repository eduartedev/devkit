use crate::error::Result;
use crate::paths::{AppConfig, Paths};
use std::fs;
use std::path::Path;

pub fn write_php_ini(php_dir: &Path, cfg: &AppConfig) -> Result<()> {
    let dest = php_dir.join("php.ini");
    let src = if php_dir.join("php.ini-development").exists() {
        php_dir.join("php.ini-development")
    } else {
        php_dir.join("php.ini-production")
    };
    let mut body = if src.exists() {
        fs::read_to_string(&src)?
    } else {
        String::new()
    };
    let ext = php_dir.join("ext");
    let ext_posix = ext.to_string_lossy().replace('\\', "/");
    let patches = format!(
        r#"
; DevKit
extension_dir = "{ext_posix}"
cgi.force_redirect = 0
cgi.fix_pathinfo = 1
fastcgi.impersonate = 1
fastcgi.logging = 0
SMTP = {smtp_host}
smtp_port = {smtp_port}
date.timezone = UTC
upload_max_filesize = 64M
post_max_size = 64M
memory_limit = 256M
display_errors = On
error_reporting = E_ALL
extension=mysqli
extension=pdo_mysql
extension=openssl
extension=mbstring
extension=curl
extension=fileinfo
extension=zip
extension=gd
extension=intl
extension=exif
"#,
        smtp_host = cfg.bind,
        smtp_port = cfg.mailpit_smtp,
    );
    if !body.contains("; DevKit") {
        body.push_str(&patches);
    }
    fs::write(dest, body)?;
    Ok(())
}

pub fn write_mysql_ini(paths: &Paths, cfg: &AppConfig) -> Result<()> {
    let datadir = paths.posix(&paths.data.join("mysql"));
    let basedir = paths.posix(&paths.bin.join("mariadb"));
    let body = format!(
        r#"[mysqld]
basedir={basedir}
datadir={datadir}
port={port}
bind-address={bind}
skip-networking=0
innodb_buffer_pool_size=128M
character-set-server=utf8mb4
collation-server=utf8mb4_unicode_ci
max_allowed_packet=64M

[client]
host={bind}
port={port}
user=root
"#,
        bind = cfg.bind,
        port = cfg.mysql_port,
    );
    crate::bind::assert_config_localhost_only(&body)?;
    fs::write(paths.mysql_cnf(), body)?;
    Ok(())
}

pub fn write_redis_conf(paths: &Paths, cfg: &AppConfig, password: &str) -> Result<()> {
    let dir = paths.posix(&paths.data.join("redis"));
    let body = format!(
        r#"bind {bind}
protected-mode yes
port {port}
requirepass {password}
dir {dir}
dbfilename dump.rdb
save ""
rename-command FLUSHALL ""
rename-command FLUSHDB ""
rename-command CONFIG ""
"#,
        bind = cfg.bind,
        port = cfg.redis_port,
        password = password,
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
  <p><a href="http://adminer.test:8080/">Adminer</a></p>
  <pre><?php echo htmlspecialchars(str_replace(PHP_EOL, "\n", php_ini_loaded_file() ?: ''), ENT_QUOTES, 'UTF-8'); ?></pre>
</body>
</html>
"#,
    )?;
    Ok(())
}

pub fn enable_php_extensions_safe(php_dir: &Path) -> Result<()> {
    write_php_ini(php_dir, &AppConfig::default())
}

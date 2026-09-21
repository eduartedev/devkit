use crate::download::{
    composer_latest, download_verified, github_latest_asset, node_lts_zip, php_nts_from_releases,
    unzip_safe,
};
use crate::error::{Error, Result};
use crate::paths::{AppConfig, Paths};
use crate::php::{write_default_index, write_mysql_ini, write_php_ini, write_redis_conf};
use crate::secrets::redis_password;
use std::fs;
use std::io::Write;

const NGINX_URL: &str = "https://nginx.org/download/nginx-1.28.2.zip";
const NGINX_SHA: &str = "6af87a4a0968111ff22a9bc75e32f4abb01ca2fe8c8ee3507d344fdbded8f7ad";
const MARIADB_URL: &str =
    "https://archive.mariadb.org/mariadb-11.4.8/winx64-packages/mariadb-11.4.8-winx64.zip";
const MARIADB_SHA: &str = "ed86e93157af46317bb49161451c2ec258498a6fa8e68ca821ef1d780d855e6b";

pub struct SetupReport {
    pub lines: Vec<String>,
}

impl SetupReport {
    fn push(&mut self, s: impl Into<String>) {
        let line = s.into();
        println!("{line}");
        self.lines.push(line);
    }
}

pub fn run_setup(paths: &Paths, cfg: &AppConfig, full: bool, php_major: Option<&str>) -> Result<SetupReport> {
    paths.ensure()?;
    write_default_index(paths)?;
    let mut report = SetupReport { lines: Vec::new() };
    let php_major = php_major.unwrap_or(cfg.php_version.as_str());

    install_nginx(paths, &mut report)?;
    install_php(paths, php_major, cfg, &mut report)?;
    if full && php_major != "8.4" {
        let _ = install_php(paths, "8.4", cfg, &mut report);
    }
    install_mariadb(paths, cfg, &mut report)?;
    install_adminer(paths, &mut report)?;

    if full {
        install_redis(paths, cfg, &mut report)?;
        install_mailpit(paths, &mut report)?;
        install_mkcert(paths, &mut report)?;
        install_node(paths, &mut report)?;
        install_composer(paths, &mut report)?;
    } else {
        let _ = install_composer(paths, &mut report);
        report.push("setup básico listo (usa --full para Redis, Mailpit, mkcert y Node)");
    }

    let _ = crate::secrets::mysql_credentials(paths)?;
    report.push(
        "MariaDB: usuario root sin contraseña por defecto (editable en el panel / secrets)",
    );
    Ok(report)
}

fn install_nginx(paths: &Paths, report: &mut SetupReport) -> Result<()> {
    let exe = paths.nginx_exe();
    if exe.exists() {
        report.push("nginx: ya instalado");
        return Ok(());
    }
    let zip = paths.tmp.join("nginx-1.28.2.zip");
    if !zip.exists() {
        report.push("nginx: descargando zip oficial…");
        download_verified(NGINX_URL, &zip, Some(NGINX_SHA))?;
    } else {
        // still verify if we already have it from a previous attempt
        let bytes = fs::read(&zip)?;
        use sha2::{Digest, Sha256};
        let actual = hex::encode(Sha256::digest(&bytes));
        if !actual.eq_ignore_ascii_case(NGINX_SHA) {
            fs::remove_file(&zip)?;
            download_verified(NGINX_URL, &zip, Some(NGINX_SHA))?;
        }
    }
    unzip_safe(&zip, &paths.bin.join("nginx"))?;
    if !exe.exists() {
        return Err(Error::msg("nginx.exe no apareció tras extraer"));
    }
    report.push("nginx: instalado");
    Ok(())
}

fn install_php(paths: &Paths, major: &str, cfg: &AppConfig, report: &mut SetupReport) -> Result<()> {
    let dir = paths.php_dir(major);
    if paths.php_cgi(major).exists() {
        write_php_ini(paths, &dir, cfg)?;
        report.push(format!("php {major}: ya instalado"));
        return Ok(());
    }
    report.push(format!("php {major}: consultando releases.json…"));
    let (ver, url, sha) = php_nts_from_releases(major)?;
    let zip = paths.tmp.join(format!("php-{ver}-nts.zip"));
    report.push(format!("php {ver}: descargando NTS…"));
    download_verified(&url, &zip, Some(&sha))?;
    unzip_safe(&zip, &dir)?;
    write_php_ini(paths, &dir, cfg)?;
    if !paths.php_cgi(major).exists() {
        return Err(Error::msg("php-cgi.exe no apareció"));
    }
    report.push(format!("php {ver}: instalado en bin/php/{major}"));
    Ok(())
}

fn install_mariadb(paths: &Paths, cfg: &AppConfig, report: &mut SetupReport) -> Result<()> {
    if paths.mysqld().exists() {
        write_mysql_ini(paths, cfg)?;
        report.push("mariadb: ya instalado");
        return Ok(());
    }
    let zip = paths.tmp.join("mariadb-11.4.8-winx64.zip");
    report.push("mariadb: descargando zip oficial (puede tardar)…");
    download_verified(MARIADB_URL, &zip, Some(MARIADB_SHA))?;
    unzip_safe(&zip, &paths.bin.join("mariadb"))?;
    write_mysql_ini(paths, cfg)?;
    if !paths.mysqld().exists() {
        return Err(Error::msg("mysqld.exe no apareció"));
    }
    report.push("mariadb: instalado");
    Ok(())
}

fn install_adminer(paths: &Paths, report: &mut SetupReport) -> Result<()> {
    let dest_dir = paths.www.join("adminer");
    fs::create_dir_all(&dest_dir)?;
    let dest = dest_dir.join("index.php");
    if dest.exists() {
        report.push("adminer: ya instalado");
        return Ok(());
    }
    match github_latest_asset("vrana/adminer", &["adminer-", ".php"]) {
        Ok((url, sha)) => {
            // skip adminer-*.zip / plugin packs: prefer a single php that is not "adminer-plugin"
            if url.contains("plugin") || url.ends_with(".zip") {
                fallback_adminer(&dest)?;
            } else {
                download_verified(&url, &dest, sha.as_deref())?;
            }
        }
        Err(_) => fallback_adminer(&dest)?,
    }
    report.push("adminer: www/adminer/index.php");
    Ok(())
}

fn fallback_adminer(dest: &std::path::Path) -> Result<()> {
    let url = "https://github.com/vrana/adminer/releases/download/v5.3.0/adminer-5.3.0.php";
    download_verified(url, dest, None)?;
    Ok(())
}

fn install_redis(paths: &Paths, cfg: &AppConfig, report: &mut SetupReport) -> Result<()> {
    if paths.redis_server().exists() {
        let pw = redis_password(paths)?;
        write_redis_conf(paths, cfg, &pw)?;
        report.push("redis: ya instalado");
        return Ok(());
    }
    report.push("redis: buscando release Windows (tporadowski)…");
    let (url, sha) = github_latest_asset("tporadowski/redis", &["redis-x64", ".zip"])
        .or_else(|_| github_latest_asset("tporadowski/redis", &[".zip"]))?;
    let zip = paths.tmp.join("redis-win.zip");
    download_verified(&url, &zip, sha.as_deref())?;
    unzip_safe(&zip, &paths.bin.join("redis"))?;
    let pw = redis_password(paths)?;
    write_redis_conf(paths, cfg, &pw)?;
    report.push("redis: instalado (bind 127.0.0.1 + requirepass)");
    Ok(())
}

fn install_mailpit(paths: &Paths, report: &mut SetupReport) -> Result<()> {
    if paths.mailpit().exists() {
        report.push("mailpit: ya instalado");
        return Ok(());
    }
    let (url, sha) = github_latest_asset("axllent/mailpit", &["windows", "amd64", ".zip"])
        .or_else(|_| github_latest_asset("axllent/mailpit", &["windows", "amd64"]))?;
    let zip = paths.tmp.join("mailpit.zip");
    download_verified(&url, &zip, sha.as_deref())?;
    unzip_safe(&zip, &paths.bin.join("mailpit"))?;
    report.push("mailpit: instalado");
    Ok(())
}

fn install_mkcert(paths: &Paths, report: &mut SetupReport) -> Result<()> {
    if paths.mkcert().exists() {
        report.push("mkcert: ya instalado");
        return Ok(());
    }
    let (url, sha) = github_latest_asset("FiloSottile/mkcert", &["windows-amd64"])?;
    download_verified(&url, &paths.mkcert(), sha.as_deref())?;
    report.push("mkcert: instalado");
    Ok(())
}

fn install_node(paths: &Paths, report: &mut SetupReport) -> Result<()> {
    if paths.bin.join("node").join("node.exe").exists() {
        report.push("node: ya instalado");
        return Ok(());
    }
    report.push("node: consultando LTS…");
    let (ver, url, sha) = node_lts_zip()?;
    let zip = paths.tmp.join(format!("node-{ver}-win-x64.zip"));
    download_verified(&url, &zip, Some(&sha))?;
    unzip_safe(&zip, &paths.bin.join("node"))?;
    report.push(format!("node {ver}: instalado"));
    Ok(())
}

fn install_composer(paths: &Paths, report: &mut SetupReport) -> Result<()> {
    if paths.composer().exists() {
        report.push("composer: ya instalado");
        return Ok(());
    }
    match composer_latest() {
        Ok((url, sha)) => {
            download_verified(&url, &paths.composer(), Some(&sha))?;
        }
        Err(_) => {
            download_verified(
                "https://getcomposer.org/download/latest-stable/composer.phar",
                &paths.composer(),
                None,
            )?;
        }
    }
    report.push("composer: instalado");
    Ok(())
}

pub fn list_php_versions(paths: &Paths) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let dir = paths.bin.join("php");
    if !dir.exists() {
        return Ok(out);
    }
    for e in fs::read_dir(dir)? {
        let e = e?;
        if e.path().is_dir() && e.path().join("php-cgi.exe").exists() {
            out.push(e.file_name().to_string_lossy().into_owned());
        }
    }
    out.sort();
    Ok(out)
}

pub fn log_setup(paths: &Paths, report: &SetupReport) -> Result<()> {
    let mut f = fs::File::create(paths.logs.join("setup.log"))?;
    for line in &report.lines {
        writeln!(f, "{line}")?;
    }
    Ok(())
}

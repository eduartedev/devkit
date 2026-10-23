use crate::download::{
    composer_latest, download_verified, git_mingit_zip, github_latest_asset, node_lts_zip,
    php_nts_from_releases, unzip_safe,
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
    let offline = paths.root.join("offline");
    let bundled = offline_bundle_ready(&offline);
    let online = bundled && crate::download::internet_available();
    let mut used_bundle = false;

    if !full && prefer_bundled_stack(online, bundled) {
        report.push("sin internet: usando Nginx, PHP, MariaDB, Composer, Node y Git del instalador");
        import_offline_bundle(&offline, paths)?;
        used_bundle = true;
    } else if !full && bundled {
        report.push("hay internet: descargando Nginx, PHP, MariaDB, Composer, Node y Git");
    }

    if let Err(err) = install_components(paths, cfg, full, php_major, &mut report) {
        if !full && bundled && !used_bundle {
            report.push("la descarga falló: usando los binarios del instalador");
            import_offline_bundle(&offline, paths)?;
            install_components(paths, cfg, full, php_major, &mut report)?;
        } else {
            return Err(err);
        }
    }

    if bundled {
        let _ = fs::remove_dir_all(&offline);
    }
    Ok(report)
}

fn install_components(
    paths: &Paths,
    cfg: &AppConfig,
    full: bool,
    php_major: &str,
    report: &mut SetupReport,
) -> Result<()> {
    install_nginx(paths, report)?;
    install_php(paths, php_major, cfg, report)?;
    if full {
        for extra in crate::php_version::SUPPORTED_PHP_MAJORS {
            if *extra != php_major {
                let _ = install_php(paths, extra, cfg, report);
            }
        }
    }
    install_mariadb(paths, cfg, report)?;
    install_adminer(paths, report)?;
    install_node(paths, report)?;
    install_git(paths, report)?;

    if full {
        install_redis(paths, cfg, report)?;
        install_mailpit(paths, report)?;
        install_mkcert(paths, report)?;
        install_composer(paths, report)?;
    } else {
        install_composer(paths, report)?;
        report.push("setup básico listo (Nginx, PHP, MariaDB, Composer, Node y Git). Setup completo añade Redis, Mailpit y mkcert");
    }

    let _ = crate::secrets::mysql_credentials(paths)?;
    report.push(
        "MariaDB: usuario root sin contraseña por defecto (editable en el panel / secrets)",
    );
    Ok(())
}

/// Con internet se descarga. Sin internet, y si el instalador trae `offline/`, se usa esa copia.
pub fn prefer_bundled_stack(internet: bool, bundle_present: bool) -> bool {
    bundle_present && !internet
}

fn offline_bundle_ready(offline: &std::path::Path) -> bool {
    offline.join("bin").join("nginx").join("nginx.exe").is_file()
}

fn import_offline_bundle(offline: &std::path::Path, paths: &Paths) -> Result<()> {
    copy_dir_contents(&offline.join("bin"), &paths.bin)?;
    let www = offline.join("www");
    if www.is_dir() {
        copy_dir_contents(&www, &paths.www)?;
    }
    Ok(())
}

fn copy_dir_contents(src: &std::path::Path, dest: &std::path::Path) -> Result<()> {
    if !src.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let to = dest.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_dir_contents(&entry.path(), &to)?;
        } else {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
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
    if !crate::php_version::supported_php_major(major) {
        return Err(Error::msg(format!(
            "PHP {major} no soportado; usa {}",
            crate::php_version::SUPPORTED_PHP_MAJORS.join(", ")
        )));
    }
    let dir = paths.php_dir(major);
    if paths.php_cgi(major).exists() {
        let installed = crate::php_version::read_installed_version(&paths.php_exe(major))
            .or_else(|_| crate::php_version::read_installed_version(&paths.php_cgi(major)));
        match installed {
            Ok(ver) if crate::php_version::meets_php2shell_minimum(&ver, major) => {
                write_php_ini(paths, &dir, cfg)?;
                report.push(format!("php {major}: ya instalado ({ver}, parche php2shell OK)"));
                return Ok(());
            }
            Ok(ver) => {
                let min = crate::php_version::minimum_secure_patch(major).unwrap_or("?");
                report.push(format!(
                    "php {major}: {ver} < {min} (php2shell); actualizando…"
                ));
                fs::remove_dir_all(&dir)?;
            }
            Err(_) => {
                write_php_ini(paths, &dir, cfg)?;
                report.push(format!("php {major}: ya instalado"));
                return Ok(());
            }
        }
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
    if !paths.node_exe().exists() {
        return Err(Error::msg("node.exe no apareció tras extraer"));
    }
    report.push(format!("node {ver}: instalado"));
    Ok(())
}

fn install_git(paths: &Paths, report: &mut SetupReport) -> Result<()> {
    if paths.git_exe().is_file() {
        report.push("git: ya instalado");
        return Ok(());
    }
    report.push("git: consultando última estable…");
    let (url, sha) = git_mingit_zip()?;
    let zip = paths.tmp.join("MinGit-64-bit.zip");
    download_verified(&url, &zip, Some(&sha))?;
    unzip_safe(&zip, &paths.bin.join("git"))?;
    if !paths.git_exe().is_file() {
        return Err(Error::msg("git.exe no apareció en bin/git"));
    }
    report.push("git: instalado");
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn bundle_is_used_only_when_offline() {
        assert!(prefer_bundled_stack(false, true));
        assert!(!prefer_bundled_stack(true, true));
        assert!(!prefer_bundled_stack(false, false));
        assert!(!prefer_bundled_stack(true, false));
    }

    #[test]
    fn offline_bundle_requires_nginx_exe() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!offline_bundle_ready(dir.path()));
        let php = dir.path().join("bin").join("php").join("8.3");
        fs::create_dir_all(&php).unwrap();
        fs::write(php.join("php-cgi.exe"), b"php").unwrap();
        assert!(!offline_bundle_ready(dir.path()));
        let nginx = dir.path().join("bin").join("nginx");
        fs::create_dir_all(&nginx).unwrap();
        fs::write(nginx.join("nginx.exe"), b"nginx").unwrap();
        assert!(offline_bundle_ready(dir.path()));
    }

    #[test]
    fn import_offline_copies_nginx_into_bin() {
        let root = std::env::temp_dir().join(format!("devkit-offline-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let offline_nginx = root.join("offline").join("bin").join("nginx");
        fs::create_dir_all(&offline_nginx).unwrap();
        let mut exe = fs::File::create(offline_nginx.join("nginx.exe")).unwrap();
        exe.write_all(b"nginx").unwrap();
        let paths = Paths::from_root(root.join("kit"));
        fs::create_dir_all(&paths.bin).unwrap();
        import_offline_bundle(&root.join("offline"), &paths).unwrap();
        let copied = fs::read(paths.nginx_exe()).unwrap();
        assert_eq!(copied, b"nginx");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn import_offline_copies_stack_and_adminer() {
        let root = tempfile::tempdir().unwrap();
        let offline = root.path().join("offline");
        write_file(&offline.join("bin").join("nginx").join("nginx.exe"), b"nginx");
        write_file(
            &offline.join("bin").join("php").join("8.3").join("php-cgi.exe"),
            b"php",
        );
        write_file(
            &offline.join("bin").join("mariadb").join("bin").join("mysqld.exe"),
            b"mysqld",
        );
        write_file(
            &offline.join("bin").join("composer").join("composer.phar"),
            b"composer",
        );
        write_file(&offline.join("www").join("adminer").join("index.php"), b"<?php");
        let paths = Paths::from_root(root.path().join("kit"));
        import_offline_bundle(&offline, &paths).unwrap();
        assert_eq!(fs::read(paths.nginx_exe()).unwrap(), b"nginx");
        assert_eq!(fs::read(paths.php_cgi("8.3")).unwrap(), b"php");
        assert_eq!(fs::read(paths.mysqld()).unwrap(), b"mysqld");
        assert_eq!(fs::read(paths.composer()).unwrap(), b"composer");
        assert_eq!(
            fs::read(paths.www.join("adminer").join("index.php")).unwrap(),
            b"<?php"
        );
    }

    #[test]
    fn import_offline_without_www_still_copies_bin() {
        let root = tempfile::tempdir().unwrap();
        let offline = root.path().join("offline");
        write_file(&offline.join("bin").join("nginx").join("nginx.exe"), b"nginx");
        let paths = Paths::from_root(root.path().join("kit"));
        import_offline_bundle(&offline, &paths).unwrap();
        assert_eq!(fs::read(paths.nginx_exe()).unwrap(), b"nginx");
        assert!(!paths.www.join("adminer").exists());
    }

    #[test]
    fn basic_setup_skips_download_when_binaries_exist() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::from_root(root.path().to_path_buf());
        paths.ensure().unwrap();
        let cfg = AppConfig::default();
        write_file(&paths.nginx_exe(), b"nginx");
        write_file(&paths.php_cgi(&cfg.php_version), b"php");
        write_file(&paths.mysqld(), b"mysqld");
        write_file(&paths.composer(), b"composer");
        write_file(&paths.bin.join("node").join("node.exe"), b"node");
        write_file(&paths.bin.join("git").join("cmd").join("git.exe"), b"git");
        write_file(&paths.www.join("adminer").join("index.php"), b"<?php");

        let report = run_setup(&paths, &cfg, false, None).unwrap();
        let text = report.lines.join("\n");
        assert!(text.contains("nginx: ya instalado"), "{text}");
        assert!(text.contains("php 8.3: ya instalado"), "{text}");
        assert!(text.contains("mariadb: ya instalado"), "{text}");
        assert!(text.contains("adminer: ya instalado"), "{text}");
        assert!(text.contains("composer: ya instalado"), "{text}");
        assert!(text.contains("node: ya instalado"), "{text}");
        assert!(text.contains("git: ya instalado"), "{text}");
        assert!(
            !text.to_ascii_lowercase().contains("descargando"),
            "{text}"
        );
        assert_eq!(fs::read(paths.nginx_exe()).unwrap(), b"nginx");
        assert_eq!(fs::read(paths.composer()).unwrap(), b"composer");
    }

    #[test]
    fn git_exe_finds_cmd_then_bin_then_flat_copy() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::from_root(root.path().join("kit"));
        assert_eq!(
            paths.git_exe(),
            paths.bin.join("git").join("cmd").join("git.exe")
        );
        write_file(&paths.bin.join("git").join("git.exe"), b"git");
        assert_eq!(paths.git_exe(), paths.bin.join("git").join("git.exe"));
        write_file(&paths.bin.join("git").join("bin").join("git.exe"), b"git");
        assert_eq!(
            paths.git_exe(),
            paths.bin.join("git").join("bin").join("git.exe")
        );
        write_file(&paths.bin.join("git").join("cmd").join("git.exe"), b"git");
        assert_eq!(
            paths.git_exe(),
            paths.bin.join("git").join("cmd").join("git.exe")
        );
    }

    fn write_file(path: &std::path::Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, bytes).unwrap();
    }
}

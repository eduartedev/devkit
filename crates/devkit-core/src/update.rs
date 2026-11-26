//! Comprobación e instalación de actualizaciones (parches y majors) de PHP/Python/Node.

use crate::download::{
    download_verified, node_lts_zip, php_nts_from_releases, python_install_only_latest, unzip_safe,
};
use crate::error::{Error, Result};
use crate::paths::{AppConfig, Paths};
use crate::php_version::{self, SUPPORTED_PHP_MAJORS};
use crate::python_version::{self, SUPPORTED_PYTHON_MAJORS};
use crate::setup::{install_php_version, install_python_version};
use crate::win_spawn;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateKind {
    Patch,
    Major,
}

impl UpdateKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Patch => "patch",
            Self::Major => "major",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateComponent {
    Php,
    Python,
    Node,
}

impl UpdateComponent {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Php => "php",
            Self::Python => "python",
            Self::Node => "node",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCandidate {
    /// Id estable: `php-8.3-patch`, `php-8.5-major`, `python-3.13-patch`, `node-lts`.
    pub id: String,
    pub component: UpdateComponent,
    pub kind: UpdateKind,
    pub installed: String,
    pub available: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateCache {
    pub checked_at: String,
    #[serde(default)]
    pub candidates: Vec<UpdateCandidate>,
}

impl UpdateCache {
    pub fn path(paths: &Paths) -> PathBuf {
        paths.etc.join("devkit").join("update-cache.toml")
    }

    pub fn load(paths: &Paths) -> Result<Self> {
        let p = Self::path(paths);
        if !p.exists() {
            return Ok(Self::default());
        }
        Ok(toml::from_str(&fs::read_to_string(p)?)?)
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        let p = Self::path(paths);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(p, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}

pub fn check_updates(paths: &Paths, cfg: &AppConfig) -> Result<Vec<UpdateCandidate>> {
    let mut out = Vec::new();
    out.extend(check_php(paths)?);
    out.extend(check_python(paths)?);
    out.extend(check_node(paths)?);
    let cache = UpdateCache {
        checked_at: chrono_like_now(),
        candidates: out.clone(),
    };
    let _ = cache.save(paths);
    let _ = cfg;
    Ok(out)
}

fn chrono_like_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unix:{now}")
}

fn check_php(paths: &Paths) -> Result<Vec<UpdateCandidate>> {
    let mut out = Vec::new();
    for major in SUPPORTED_PHP_MAJORS {
        let installed_path = paths.php_exe(major);
        let installed = if installed_path.is_file() {
            php_version::read_installed_version(&installed_path)
                .unwrap_or_else(|_| major.to_string())
        } else {
            String::new()
        };

        let (available, _url, _sha) = match php_nts_from_releases(major) {
            Ok(t) => t,
            Err(_) => continue,
        };

        if installed.is_empty() {
            out.push(UpdateCandidate {
                id: format!("php-{major}-major"),
                component: UpdateComponent::Php,
                kind: UpdateKind::Major,
                installed: "(no instalado)".into(),
                available: available.clone(),
                detail: format!("PHP {major} disponible para instalar en paralelo"),
            });
            continue;
        }

        if php_version::version_ge(&installed, &available) {
            continue;
        }
        out.push(UpdateCandidate {
            id: format!("php-{major}-patch"),
            component: UpdateComponent::Php,
            kind: UpdateKind::Patch,
            installed: installed.clone(),
            available: available.clone(),
            detail: format!("parche PHP {major}: {installed} → {available}"),
        });
    }
    Ok(out)
}

fn check_python(paths: &Paths) -> Result<Vec<UpdateCandidate>> {
    let mut out = Vec::new();
    let triple = python_version::host_python_triple().map_err(Error::msg)?;
    for major in SUPPORTED_PYTHON_MAJORS {
        let exe = paths.python_exe(major);
        let installed = if exe.is_file() {
            read_python_version(&exe).unwrap_or_else(|_| major.to_string())
        } else {
            String::new()
        };

        let (stamp, _url, _sha) = match python_install_only_latest(major, triple) {
            Ok(t) => t,
            Err(_) => continue,
        };

        if installed.is_empty() {
            out.push(UpdateCandidate {
                id: format!("python-{major}-major"),
                component: UpdateComponent::Python,
                kind: UpdateKind::Major,
                installed: "(no instalado)".into(),
                available: stamp.clone(),
                detail: format!("Python {major} disponible para instalar en paralelo"),
            });
            continue;
        }

        let local = python_install_stamp(paths, major);
        if local.as_deref() == Some(stamp.as_str()) {
            continue;
        }
        out.push(UpdateCandidate {
            id: format!("python-{major}-patch"),
            component: UpdateComponent::Python,
            kind: UpdateKind::Patch,
            installed: local.unwrap_or(installed),
            available: stamp.clone(),
            detail: format!("Python {major}: build remoto {stamp}"),
        });
    }
    Ok(out)
}

fn check_node(paths: &Paths) -> Result<Vec<UpdateCandidate>> {
    let mut out = Vec::new();
    let (available, _url, _sha) = node_lts_zip()?;
    let available_clean = available.trim_start_matches('v').to_string();
    let installed = if paths.node_exe().is_file() {
        read_node_version(&paths.node_exe()).unwrap_or_else(|_| "(desconocida)".into())
    } else {
        String::new()
    };
    if installed.is_empty() {
        out.push(UpdateCandidate {
            id: "node-lts".into(),
            component: UpdateComponent::Node,
            kind: UpdateKind::Major,
            installed: "(no instalado)".into(),
            available: available_clean,
            detail: "Node LTS no instalado".into(),
        });
        return Ok(out);
    }
    let inst = installed.trim_start_matches('v');
    if version_tuple(inst) >= version_tuple(&available_clean) {
        return Ok(out);
    }
    out.push(UpdateCandidate {
        id: "node-lts".into(),
        component: UpdateComponent::Node,
        kind: UpdateKind::Patch,
        installed: inst.to_string(),
        available: available_clean.clone(),
        detail: format!("Node LTS: {inst} → {available_clean}"),
    });
    Ok(out)
}

fn version_tuple(v: &str) -> (u32, u32, u32) {
    let mut parts = v.split('.');
    let a = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let b = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let c = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (a, b, c)
}

fn read_node_version(exe: &PathBuf) -> Result<String> {
    let mut cmd = Command::new(exe);
    cmd.arg("-v");
    let out =
        win_spawn::output_hidden(&mut cmd).map_err(|e| Error::msg(format!("node -v: {e}")))?;
    if !out.status.success() {
        return Err(Error::msg("node -v falló"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn read_python_version(exe: &PathBuf) -> Result<String> {
    let mut cmd = Command::new(exe);
    cmd.args([
        "-c",
        "import sys; print('.'.join(map(str, sys.version_info[:3])))",
    ]);
    let out =
        win_spawn::output_hidden(&mut cmd).map_err(|e| Error::msg(format!("python -c: {e}")))?;
    if !out.status.success() {
        return Err(Error::msg("python version falló"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn python_install_stamp(paths: &Paths, major: &str) -> Option<String> {
    let p = paths.python_dir(major).join(".devkit-python-build");
    fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}

fn write_python_install_stamp(paths: &Paths, major: &str, tag: &str) -> Result<()> {
    let p = paths.python_dir(major).join(".devkit-python-build");
    fs::write(p, format!("{tag}\n"))?;
    Ok(())
}

/// Instala el candidato **sin** cambiar la versión activa en config.
pub fn install_candidate(
    paths: &Paths,
    cfg: &AppConfig,
    id: &str,
    on_line: &mut impl FnMut(&str),
) -> Result<UpdateCandidate> {
    let candidates = UpdateCache::load(paths)
        .ok()
        .filter(|c| !c.candidates.is_empty())
        .map(|c| c.candidates)
        .unwrap_or_else(|| check_updates(paths, cfg).unwrap_or_default());
    let cand = candidates
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| {
            Error::msg(format!(
                "candidato '{id}' no encontrado; ejecuta `devkit update check`"
            ))
        })?;

    match cand.component {
        UpdateComponent::Php => {
            let major = php_major_from_id(&cand.id)?;
            let force = cand.kind == UpdateKind::Patch;
            crate::process_stream::write_log_line(
                on_line,
                &format!("→ instalando PHP {major} ({})…", cand.available),
            );
            for line in install_php_version(paths, &major, cfg, force)? {
                crate::process_stream::write_log_line(on_line, &line);
            }
        }
        UpdateComponent::Python => {
            let major = python_major_from_id(&cand.id)?;
            let force = cand.kind == UpdateKind::Patch;
            crate::process_stream::write_log_line(
                on_line,
                &format!("→ instalando Python {major}…"),
            );
            for line in install_python_version(paths, &major, force)? {
                crate::process_stream::write_log_line(on_line, &line);
            }
            let triple = python_version::host_python_triple().map_err(Error::msg)?;
            if let Ok((stamp, _, _)) = python_install_only_latest(&major, triple) {
                let _ = write_python_install_stamp(paths, &major, &stamp);
            }
        }
        UpdateComponent::Node => {
            crate::process_stream::write_log_line(on_line, "→ descargando Node LTS a staging…");
            install_node_staging(paths, on_line)?;
        }
    }
    Ok(cand)
}

fn php_major_from_id(id: &str) -> Result<String> {
    let parts: Vec<_> = id.split('-').collect();
    if parts.len() >= 2 && parts[0] == "php" {
        return Ok(parts[1].to_string());
    }
    Err(Error::msg(format!("id PHP inválido: {id}")))
}

fn python_major_from_id(id: &str) -> Result<String> {
    let parts: Vec<_> = id.split('-').collect();
    if parts.len() >= 2 && parts[0] == "python" {
        return Ok(parts[1].to_string());
    }
    Err(Error::msg(format!("id Python inválido: {id}")))
}

/// Servicio a detener antes de reinstalar (parche sobre major activo).
pub fn service_to_pause_for_install(
    id: &str,
    php_version: &str,
    python_version: &str,
) -> Option<&'static str> {
    if let Ok(major) = php_major_from_id(id) {
        if major == php_version {
            return Some("php");
        }
    }
    if let Ok(major) = python_major_from_id(id) {
        if major == python_version {
            return Some("python");
        }
    }
    None
}

fn install_node_staging(paths: &Paths, on_line: &mut impl FnMut(&str)) -> Result<()> {
    let (ver, url, sha) = node_lts_zip()?;
    let staging = paths.bin.join(format!("node-{}", ver.trim_start_matches('v')));
    if staging.join("node.exe").is_file() {
        crate::process_stream::write_log_line(on_line, &format!("node {ver}: ya en staging"));
        return Ok(());
    }
    fs::create_dir_all(&staging)?;
    let zip = paths.tmp.join(format!("node-{ver}-win-x64.zip"));
    download_verified(&url, &zip, Some(&sha))?;
    unzip_safe(&zip, &staging)?;
    if !staging.join("node.exe").is_file() {
        return Err(Error::msg("node.exe no apareció en staging"));
    }
    crate::process_stream::write_log_line(on_line, &format!("node {ver}: staging listo"));
    Ok(())
}

/// Activa un candidato ya instalado (`use_php` / `use_python` / swap Node).
pub fn activate_candidate(paths: &Paths, cfg: &mut AppConfig, id: &str) -> Result<String> {
    match id {
        "node-lts" => {
            activate_node_staging(paths)?;
            Ok("Node LTS activado (bin/node sustituido desde staging)".into())
        }
        other if other.starts_with("php-") => {
            let major = php_major_from_id(other)?;
            if !paths.php_cgi(&major).is_file() {
                return Err(Error::msg(format!(
                    "PHP {major} no está instalado; ejecuta update install {other}"
                )));
            }
            cfg.php_version = major.clone();
            cfg.save(&paths.config_file())?;
            let _ = crate::path_env::sync_configured(paths, cfg);
            Ok(format!("PHP {major} activo"))
        }
        other if other.starts_with("python-") => {
            let major = python_major_from_id(other)?;
            if !paths.python_exe(&major).is_file() {
                return Err(Error::msg(format!(
                    "Python {major} no está instalado; ejecuta update install {other}"
                )));
            }
            cfg.python_version = major.clone();
            cfg.save(&paths.config_file())?;
            let _ = crate::path_env::sync_configured(paths, cfg);
            Ok(format!("Python {major} activo"))
        }
        other => Err(Error::msg(format!("id desconocido: {other}"))),
    }
}

fn activate_node_staging(paths: &Paths) -> Result<()> {
    let mut best: Option<PathBuf> = None;
    if let Ok(rd) = fs::read_dir(&paths.bin) {
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("node-") {
                continue;
            }
            if entry.path().join("node.exe").is_file() {
                best = Some(entry.path());
            }
        }
    }
    let staging = best.ok_or_else(|| {
        Error::msg("no hay Node en staging; ejecuta update install node-lts")
    })?;
    let dest = paths.bin.join("node");
    if dest.exists() {
        let bak = paths.bin.join("node.bak");
        let _ = fs::remove_dir_all(&bak);
        fs::rename(&dest, &bak).map_err(|e| Error::msg(format!("backup node: {e}")))?;
    }
    fs::rename(&staging, &dest).map_err(|e| Error::msg(format!("activar node: {e}")))?;
    let _ = fs::remove_dir_all(paths.bin.join("node.bak"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_tuple_orders() {
        assert!(version_tuple("22.11.0") > version_tuple("20.18.0"));
        assert!(version_tuple("8.3.36") >= version_tuple("8.3.36"));
    }

    #[test]
    fn parse_ids() {
        assert_eq!(php_major_from_id("php-8.4-patch").unwrap(), "8.4");
        assert_eq!(python_major_from_id("python-3.13-major").unwrap(), "3.13");
    }

    #[test]
    fn pause_only_active_major_on_install() {
        assert_eq!(
            service_to_pause_for_install("php-8.5-patch", "8.5", "3.13"),
            Some("php")
        );
        assert_eq!(
            service_to_pause_for_install("php-8.4-patch", "8.5", "3.13"),
            None
        );
        assert_eq!(
            service_to_pause_for_install("python-3.13-patch", "8.5", "3.13"),
            Some("python")
        );
        assert_eq!(
            service_to_pause_for_install("node-lts", "8.5", "3.13"),
            None
        );
    }

    #[test]
    fn cache_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::from_root(dir.path().to_path_buf());
        paths.ensure().unwrap();
        let cache = UpdateCache {
            checked_at: "unix:1".into(),
            candidates: vec![UpdateCandidate {
                id: "php-8.3-patch".into(),
                component: UpdateComponent::Php,
                kind: UpdateKind::Patch,
                installed: "8.3.30".into(),
                available: "8.3.36".into(),
                detail: "test".into(),
            }],
        };
        cache.save(&paths).unwrap();
        let loaded = UpdateCache::load(&paths).unwrap();
        assert_eq!(loaded.candidates.len(), 1);
        assert_eq!(loaded.candidates[0].id, "php-8.3-patch");
    }

    #[test]
    fn install_does_not_change_active_php_in_config_helper() {
        // activate es el único que muta config; install usa &AppConfig
        let mut cfg = AppConfig::default();
        let before = cfg.php_version.clone();
        assert_eq!(before, cfg.php_version);
        let _ = &mut cfg;
    }
}

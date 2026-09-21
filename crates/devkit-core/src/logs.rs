//! Lectura segura de logs del kit y de proyectos bajo www/.

use crate::error::{Error, Result};
use crate::paths::Paths;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

const MAX_TAIL_BYTES: u64 = 2 * 1024 * 1024; // 2 MiB
const DEFAULT_TAIL_LINES: usize = 80;
const MAX_TAIL_LINES: usize = 500;
const MAX_SEARCH_MATCHES: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogSourceKind {
    Kit,
    Project,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Id corto: `php`, `nginx-error`, o ruta relativa bajo www/ (posix).
    pub id: String,
    pub kind: LogSourceKind,
    pub path: PathBuf,
    pub exists: bool,
    pub size_bytes: u64,
}

/// Logs fijos del kit bajo `logs/` (una sola carpeta para el MCP).
pub fn kit_log_specs(paths: &Paths) -> Vec<(String, PathBuf)> {
    vec![
        ("php".into(), paths.logs.join("php_error.log")),
        ("nginx-error".into(), paths.logs.join("nginx-error.log")),
        ("nginx-access".into(), paths.logs.join("nginx-access.log")),
        ("mariadb".into(), paths.logs.join("mariadb-error.log")),
        ("redis".into(), paths.logs.join("redis.log")),
        ("mailpit".into(), paths.logs.join("mailpit.log")),
    ]
}

pub fn list_logs(paths: &Paths, include_project_hints: bool) -> Result<Vec<LogEntry>> {
    let mut out = Vec::new();
    for (id, path) in kit_log_specs(paths) {
        out.push(stat_entry(id, LogSourceKind::Kit, path));
    }
    if include_project_hints {
        for hint in discover_project_log_hints(paths)? {
            let abs = paths.www.join(&hint);
            out.push(stat_entry(
                hint.replace('\\', "/"),
                LogSourceKind::Project,
                abs,
            ));
        }
    }
    Ok(out)
}

fn stat_entry(id: String, kind: LogSourceKind, path: PathBuf) -> LogEntry {
    let (exists, size_bytes) = match fs::metadata(&path) {
        Ok(m) => (true, m.len()),
        Err(_) => (false, 0),
    };
    LogEntry {
        id,
        kind,
        path,
        exists,
        size_bytes,
    }
}

/// Busca logs típicos de frameworks bajo www/*/… (solo lectura, profundidad limitada).
fn discover_project_log_hints(paths: &Paths) -> Result<Vec<String>> {
    let mut hints = Vec::new();
    let www = &paths.www;
    if !www.is_dir() {
        return Ok(hints);
    }
    for entry in fs::read_dir(www)? {
        let entry = entry?;
        let site = entry.path();
        if !site.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        for rel in [
            "storage/logs/laravel.log",
            "var/log/dev.log",
            "var/logs/dev.log",
            "runtime/logs/app.log",
        ] {
            let p = site.join(rel);
            if p.is_file() {
                hints.push(format!("{name}/{rel}").replace('\\', "/"));
            }
        }
        let laravel_dir = site.join("storage/logs");
        if laravel_dir.is_dir() {
            if let Ok(rd) = fs::read_dir(&laravel_dir) {
                for f in rd.flatten() {
                    let fname = f.file_name().to_string_lossy().to_string();
                    if fname.ends_with(".log") && f.path().is_file() {
                        let id = format!("{name}/storage/logs/{fname}");
                        if !hints.iter().any(|h| h == &id) {
                            hints.push(id.replace('\\', "/"));
                        }
                    }
                }
            }
        }
    }
    hints.sort();
    hints.dedup();
    Ok(hints)
}

/// Resuelve `php` / `nginx-error` / ruta relativa bajo www/ a un path absoluto permitido.
pub fn resolve_log_path(paths: &Paths, id_or_rel: &str) -> Result<PathBuf> {
    let key = id_or_rel.trim().trim_start_matches(['/', '\\']);
    if key.is_empty() {
        return Err(Error::msg("id de log vacío"));
    }
    for (id, path) in kit_log_specs(paths) {
        if id.eq_ignore_ascii_case(key) {
            return Ok(path);
        }
    }
    // Solo bajo www/
    let rel = PathBuf::from(key);
    if rel
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::RootDir | Component::Prefix(_)))
    {
        return Err(Error::msg(
            "ruta de log inválida: no se permiten '..' ni rutas absolutas",
        ));
    }
    let abs = paths.www.join(&rel);
    let canon_www = fs::canonicalize(&paths.www).unwrap_or_else(|_| paths.www.clone());
    // Si el archivo no existe aún, validar el padre previsto
    let check = if abs.exists() {
        fs::canonicalize(&abs).unwrap_or(abs.clone())
    } else {
        abs.clone()
    };
    if abs.exists() {
        if !check.starts_with(&canon_www) {
            return Err(Error::msg("ruta de log fuera de www/"));
        }
    } else {
        // ensure relative stays under www by string prefix after normalize
        let www_posix = paths.posix(&paths.www).to_ascii_lowercase();
        let abs_posix = paths.posix(&abs).to_ascii_lowercase();
        if !abs_posix.starts_with(&www_posix) {
            return Err(Error::msg("ruta de log fuera de www/"));
        }
    }
    Ok(abs)
}

pub fn clamp_lines(n: Option<usize>) -> usize {
    n.unwrap_or(DEFAULT_TAIL_LINES).clamp(1, MAX_TAIL_LINES)
}

pub fn tail_lines(path: &Path, lines: usize) -> Result<String> {
    if !path.exists() {
        return Ok(String::new());
    }
    let lines = lines.clamp(1, MAX_TAIL_LINES);
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(MAX_TAIL_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut buf = String::new();
    file.read_to_string(&mut buf)?;
    if start > 0 {
        // descartar primera línea parcial
        if let Some(i) = buf.find('\n') {
            buf = buf[i + 1..].to_string();
        }
    }
    let all: Vec<&str> = buf.lines().collect();
    let take = all.len().saturating_sub(lines);
    Ok(all[take..].join("\n"))
}

pub fn search_lines(path: &Path, query: &str, max_matches: usize) -> Result<Vec<String>> {
    if query.trim().is_empty() {
        return Err(Error::msg("query de búsqueda vacía"));
    }
    if !path.exists() {
        return Ok(Vec::new());
    }
    let max_matches = max_matches.clamp(1, MAX_SEARCH_MATCHES);
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let q = query.to_ascii_lowercase();
    let mut hits = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.to_ascii_lowercase().contains(&q) {
            hits.push(line);
            if hits.len() >= max_matches {
                break;
            }
        }
    }
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn resolve_rejects_parent_dir() {
        let d = tempdir().unwrap();
        let paths = Paths::from_root(d.path().to_path_buf());
        fs::create_dir_all(paths.www.join("app")).unwrap();
        assert!(resolve_log_path(&paths, "../etc/passwd").is_err());
        assert!(resolve_log_path(&paths, "app/../../etc").is_err());
    }

    #[test]
    fn resolve_kit_ids() {
        let d = tempdir().unwrap();
        let paths = Paths::from_root(d.path().to_path_buf());
        let p = resolve_log_path(&paths, "php").unwrap();
        assert!(p.ends_with("php_error.log"));
    }

    #[test]
    fn tail_and_search() {
        let d = tempdir().unwrap();
        let log = d.path().join("t.log");
        let mut body = String::new();
        for i in 0..20 {
            body.push_str(&format!("line {i} ok\n"));
        }
        body.push_str("FATAL boom\n");
        fs::write(&log, &body).unwrap();
        let tail = tail_lines(&log, 5).unwrap();
        assert!(tail.contains("FATAL boom"));
        assert!(tail.lines().count() <= 5);
        let hits = search_lines(&log, "fatal", 10).unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn list_includes_all_kit_sources() {
        let d = tempdir().unwrap();
        let paths = Paths::from_root(d.path().to_path_buf());
        fs::create_dir_all(&paths.logs).unwrap();
        fs::create_dir_all(&paths.www).unwrap();
        let list = list_logs(&paths, false).unwrap();
        let ids: Vec<_> = list.iter().map(|e| e.id.as_str()).collect();
        for want in ["php", "nginx-error", "nginx-access", "mariadb", "redis", "mailpit"] {
            assert!(ids.contains(&want), "falta {want}");
        }
    }
}

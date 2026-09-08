use crate::error::{Error, Result};
use crate::model::Site;
use crate::paths::{AppConfig, Paths};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_LABEL: usize = 63;

pub fn parse_site_name(raw: &str) -> Result<String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(Error::InvalidSiteName {
            raw: raw.to_string(),
            reason: "vacío".into(),
        });
    }
    if name.len() > MAX_LABEL {
        return Err(Error::InvalidSiteName {
            raw: raw.to_string(),
            reason: "más de 63 caracteres".into(),
        });
    }
    if name == "." || name == ".." {
        return Err(Error::InvalidSiteName {
            raw: raw.to_string(),
            reason: "no se permiten . ni ..".into(),
        });
    }
    if name.contains(['/', '\\', ' ', '"', '\'', ';', '{', '}', '\n', '\r', '\0', '@', ',', '#']) {
        return Err(Error::InvalidSiteName {
            raw: raw.to_string(),
            reason: "caracteres no permitidos".into(),
        });
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(Error::InvalidSiteName {
            raw: raw.to_string(),
            reason: "solo a-z, 0-9 y guion".into(),
        });
    }
    let lower = name.to_ascii_lowercase();
    if lower.starts_with('-') || lower.ends_with('-') {
        return Err(Error::InvalidSiteName {
            raw: raw.to_string(),
            reason: "no puede empezar o terminar con guion".into(),
        });
    }
    if !lower.chars().next().unwrap().is_ascii_alphanumeric() {
        return Err(Error::InvalidSiteName {
            raw: raw.to_string(),
            reason: "debe empezar con letra o número".into(),
        });
    }
    Ok(lower)
}

pub fn site_root(paths: &Paths, name: &str) -> Result<PathBuf> {
    let safe = parse_site_name(name)?;
    let candidate = paths.www.join(&safe);
    if candidate.exists() {
        crate::bind::ensure_under(&paths.www, &candidate)?;
    } else {
        let parent = candidate.parent().unwrap_or(&paths.www);
        crate::bind::ensure_under(&paths.www, parent)?;
        if candidate
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(Error::msg("path traversal"));
        }
    }
    Ok(candidate)
}

/// Document root para nginx: `public/` si es Laravel (u otro app con public/index.php).
pub fn detect_web_root(project: &Path) -> PathBuf {
    let public_index = project.join("public").join("index.php");
    if public_index.is_file() {
        return project.join("public");
    }
    project.to_path_buf()
}

pub fn scan_sites(paths: &Paths, cfg: &AppConfig) -> Result<Vec<Site>> {
    let mut sites = Vec::new();
    if !paths.www.exists() {
        return Ok(sites);
    }
    let mut entries: Vec<_> = fs::read_dir(&paths.www)?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let fname = entry.file_name();
        let Some(raw) = fname.to_str() else {
            continue;
        };
        let Ok(name) = parse_site_name(raw) else {
            continue;
        };
        let project = entry.path();
        let root = detect_web_root(&project);
        let hostname = format!("{name}.{}", cfg.tld);
        let url = if cfg.ssl {
            format!("https://{hostname}:{}", cfg.https_port)
        } else {
            format!("http://{hostname}:{}", cfg.http_port)
        };
        sites.push(Site {
            name,
            hostname,
            root,
            php_version: cfg.php_version.clone(),
            ssl: cfg.ssl,
            url,
        });
    }
    Ok(sites)
}

pub fn nginx_escape_server_name(name: &str) -> Result<String> {
    let safe = parse_site_name(name)?;
    Ok(safe)
}

pub fn nginx_escape_root(path: &Path) -> Result<String> {
    let s = path.to_string_lossy();
    if s.contains('"') || s.contains('\n') || s.contains('{') {
        return Err(Error::msg("ruta de root insegura para nginx"));
    }
    Ok(s.replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_names() {
        assert_eq!(parse_site_name("blog").unwrap(), "blog");
        assert_eq!(parse_site_name("Tienda-1").unwrap(), "tienda-1");
    }

    #[test]
    fn rejects_injection_and_traversal() {
        assert!(parse_site_name("../windows").is_err());
        assert!(parse_site_name("foo;server_name evil.com").is_err());
        assert!(parse_site_name("foo bar").is_err());
        assert!(parse_site_name("foo\"bar").is_err());
        assert!(parse_site_name("foo{bar").is_err());
        assert!(parse_site_name("..").is_err());
        assert!(parse_site_name(".hidden").is_err() || parse_site_name(".hidden").is_ok());
        assert!(parse_site_name("-bad").is_err());
        assert!(parse_site_name("bad-").is_err());
        assert!(parse_site_name("").is_err());
    }

    #[test]
    fn dot_hidden_rejected_by_leading() {
        // ".hidden" starts with '.' which is not alphanumeric
        assert!(parse_site_name(".hidden").is_err());
    }

    #[test]
    fn laravel_uses_public_as_web_root() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("app");
        fs::create_dir_all(project.join("public")).unwrap();
        fs::write(project.join("public").join("index.php"), "<?php").unwrap();
        fs::write(project.join("artisan"), "#!/usr/bin/env php").unwrap();
        assert_eq!(detect_web_root(&project), project.join("public"));
    }

    #[test]
    fn plain_php_uses_project_root() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("plain");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("index.php"), "<?php").unwrap();
        assert_eq!(detect_web_root(&project), project);
    }
}

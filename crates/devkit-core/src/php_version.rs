use crate::error::{Error, Result};
use std::path::Path;
use std::process::Command;

/// Parches mínimos frente a php2shell (CVE-2026-109618, CVE-2026-109619).
/// Fuente: avisos PHP / CISA, octubre 2026.
pub const PHP2SHELL_MIN: &[(&str, &str)] = &[
    ("8.5", "8.5.12"),
    ("8.4", "8.4.27"),
    ("8.3", "8.3.36"),
    ("8.2", "8.2.35"),
];

pub const SUPPORTED_PHP_MAJORS: &[&str] = &["8.3", "8.4", "8.5"];

pub fn supported_php_major(major: &str) -> bool {
    SUPPORTED_PHP_MAJORS.iter().any(|m| *m == major)
}

pub fn minimum_secure_patch(major: &str) -> Option<&'static str> {
    PHP2SHELL_MIN
        .iter()
        .find(|(m, _)| *m == major)
        .map(|(_, v)| *v)
}

/// Compara versiones `X.Y.Z` (solo numérico).
pub fn version_ge(installed: &str, minimum: &str) -> bool {
    parse_triple(installed) >= parse_triple(minimum)
}

fn parse_triple(v: &str) -> (u32, u32, u32) {
    let mut parts = v.split('.');
    let a = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let b = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let c = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (a, b, c)
}

pub fn meets_php2shell_minimum(full_version: &str, major: &str) -> bool {
    let Some(min) = minimum_secure_patch(major) else {
        return true;
    };
    version_ge(full_version, min)
}

/// Lee `PHP x.y.z` desde `php.exe -v` o `php-cgi.exe -v`.
pub fn read_installed_version(php_bin: &Path) -> Result<String> {
    let out = Command::new(php_bin)
        .arg("-v")
        .output()
        .map_err(|e| Error::msg(format!("no se pudo ejecutar {}: {e}", php_bin.display())))?;
    if !out.status.success() {
        return Err(Error::msg(format!(
            "{} -v falló (código {})",
            php_bin.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(),
            out.status.code().unwrap_or(-1)
        )));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    parse_version_line(&text).ok_or_else(|| {
        Error::msg(format!(
            "no se pudo leer versión de PHP en la salida de {}",
            php_bin.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()
        ))
    })
}

fn parse_version_line(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("PHP ") {
            let ver = rest
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim();
            if ver.contains('.') {
                return Some(ver.to_string());
            }
        }
    }
    None
}

pub fn validate_downloaded_version(full_version: &str, major: &str) -> Result<()> {
    if !supported_php_major(major) {
        return Err(Error::msg(format!(
            "PHP {major} no soportado; majors: {}",
            SUPPORTED_PHP_MAJORS.join(", ")
        )));
    }
    let min = minimum_secure_patch(major).unwrap_or(full_version);
    if !version_ge(full_version, min) {
        return Err(Error::msg(format!(
            "PHP {full_version} no alcanza el parche php2shell mínimo {min} para {major}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_ge_orders_patches() {
        assert!(version_ge("8.3.36", "8.3.36"));
        assert!(version_ge("8.3.37", "8.3.36"));
        assert!(!version_ge("8.3.35", "8.3.36"));
        assert!(version_ge("8.5.12", "8.5.12"));
        assert!(version_ge("8.4.27", "8.4.26"));
    }

    #[test]
    fn parse_version_line_from_cli_output() {
        let sample = "PHP 8.3.36 (cli) (built: Oct 21 2026)\r\nCopyright (c)";
        assert_eq!(parse_version_line(sample).as_deref(), Some("8.3.36"));
    }

    #[test]
    fn validate_download_rejects_below_php2shell_floor() {
        assert!(validate_downloaded_version("8.3.35", "8.3").is_err());
        assert!(validate_downloaded_version("8.3.36", "8.3").is_ok());
    }
}

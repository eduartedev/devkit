use crate::error::{Error, Result};
use crate::model::LOCALHOST;
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};

pub fn parse_bind_host(host: &str) -> Result<String> {
    let trimmed = host.trim();
    if trimmed.eq_ignore_ascii_case("localhost") || trimmed == LOCALHOST || trimmed == "::1" {
        return Ok(LOCALHOST.to_string());
    }
    Err(Error::BindForbidden {
        got: trimmed.to_string(),
    })
}

pub fn require_localhost(host: &str) -> Result<()> {
    parse_bind_host(host).map(|_| ())
}

/// Rechaza configs que escuchen en todas las interfaces.
pub fn assert_config_localhost_only(config: &str) -> Result<()> {
    for (idx, raw_line) in config.lines().enumerate() {
        let without_comment = raw_line.split('#').next().unwrap_or("").trim();
        if without_comment.is_empty() {
            continue;
        }
        let lower = without_comment.to_ascii_lowercase();
        if lower.contains("0.0.0.0") || (lower.contains("[::]") && !lower.contains("[::1]")) {
            return Err(Error::msg(format!(
                "línea {}: bind abierto a la red: {without_comment}",
                idx + 1
            )));
        }
        let words: Vec<&str> = lower
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
            .filter(|w| !w.is_empty())
            .collect();
        let mentions_listen = words.iter().any(|w| *w == "listen");
        let mentions_bind = words.iter().any(|w| *w == "bind" || *w == "bind-address");
        if (mentions_listen || mentions_bind)
            && !lower.contains(LOCALHOST)
            && !lower.contains("localhost")
        {
            return Err(Error::msg(format!(
                "línea {}: debe incluir 127.0.0.1: {without_comment}",
                idx + 1
            )));
        }
    }
    Ok(())
}

pub fn localhost_ip() -> IpAddr {
    IpAddr::V4(Ipv4Addr::LOCALHOST)
}

pub fn ensure_under(root: &Path, candidate: &Path) -> Result<PathBuf> {
    let root_c = dunce_canonicalize(root)?;
    let cand_c = dunce_canonicalize(candidate).unwrap_or_else(|_| candidate.to_path_buf());
    if !cand_c.starts_with(&root_c) {
        return Err(Error::msg(format!(
            "ruta fuera de {}: {}",
            root_c.display(),
            cand_c.display()
        )));
    }
    Ok(cand_c)
}

fn dunce_canonicalize(path: &Path) -> Result<PathBuf> {
    match path.canonicalize() {
        Ok(p) => Ok(strip_unc(p)),
        Err(_) => Ok(path.to_path_buf()),
    }
}

fn strip_unc(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localhost_ok() {
        assert_eq!(parse_bind_host("127.0.0.1").unwrap(), LOCALHOST);
        assert_eq!(parse_bind_host("localhost").unwrap(), LOCALHOST);
    }

    #[test]
    fn wildcard_rejected() {
        assert!(parse_bind_host("0.0.0.0").is_err());
        assert!(parse_bind_host("192.168.1.10").is_err());
    }

    #[test]
    fn config_rejects_open_listen() {
        let bad = "server { listen 80; }";
        assert!(assert_config_localhost_only(bad).is_err());
        let bad2 = "bind 0.0.0.0";
        assert!(assert_config_localhost_only(bad2).is_err());
        let good = "listen 127.0.0.1:8080;\nbind-address=127.0.0.1\n";
        assert!(assert_config_localhost_only(good).is_ok());
    }
}

use crate::error::{Error, Result};
use crate::model::Site;
use crate::model::LOCALHOST;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const BEGIN: &str = "# BEGIN DEVKIT";
const END: &str = "# END DEVKIT";

pub fn hosts_path() -> PathBuf {
    PathBuf::from(r"C:\Windows\System32\drivers\etc\hosts")
}

/// Solo entradas DevKit: no reescribe localhost global ni líneas ajenas (Laragon, etc.).
pub fn render_block(sites: &[Site]) -> String {
    let mut lines = vec![
        BEGIN.to_string(),
        "# Gestionado por DevKit — no borra el resto del archivo".into(),
    ];
    let mut seen = std::collections::BTreeSet::new();
    for site in sites {
        let host = site.hostname.trim().to_ascii_lowercase();
        if host.is_empty() || !seen.insert(host.clone()) {
            continue;
        }
        lines.push(format!("{LOCALHOST} {host}"));
    }
    lines.push(END.to_string());
    lines.join("\r\n") + "\r\n"
}

/// Sustituye únicamente el bloque DevKit; todo lo demás del archivo se conserva.
pub fn merge_hosts(original: &str, sites: &[Site]) -> String {
    let block = render_block(sites);
    if let Some((before, after)) = split_around_devkit_block(original) {
        let mut out = String::new();
        out.push_str(before.trim_end_matches(['\r', '\n']));
        if !before.trim().is_empty() {
            out.push_str("\r\n\r\n");
        }
        out.push_str(&block);
        let after = after.trim_start_matches(['\r', '\n']);
        if !after.is_empty() {
            out.push_str(after);
            if !after.ends_with('\n') {
                out.push_str("\r\n");
            }
        }
        return out;
    }

    let mut out = original.trim_end_matches(['\r', '\n']).to_string();
    if !out.is_empty() {
        out.push_str("\r\n\r\n");
    }
    out.push_str(&block);
    out
}

fn split_around_devkit_block(original: &str) -> Option<(&str, &str)> {
    let a = original.find(BEGIN)?;
    let b = original[a..].find(END).map(|i| a + i)?;
    let end = b + END.len();
    // Consume trailing newline after END so we don't leave blank gaps growing.
    let mut after_idx = end;
    let bytes = original.as_bytes();
    if after_idx < bytes.len() && bytes[after_idx] == b'\r' {
        after_idx += 1;
    }
    if after_idx < bytes.len() && bytes[after_idx] == b'\n' {
        after_idx += 1;
    }
    Some((&original[..a], &original[after_idx..]))
}

pub fn apply_hosts(sites: &[Site]) -> Result<()> {
    let path = hosts_path();
    let original = fs::read_to_string(&path).unwrap_or_default();
    let merged = merge_hosts(&original, sites);
    if hosts_content_equivalent(&original, &merged) {
        return Ok(());
    }
    match try_write_hosts(&path, &merged) {
        Ok(()) => {
            flush_dns_cache();
            Ok(())
        }
        Err(e) if is_permission_denied(&e) => {
            write_hosts_elevated(&merged)?;
            flush_dns_cache();
            Ok(())
        }
        Err(e) => Err(e),
    }
}

fn try_write_hosts(path: &Path, content: &str) -> Result<()> {
    fs::write(path, content).map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            Error::msg(
                "sin permiso para editar hosts (se pedirá UAC). Laragon y el resto del archivo no se tocan.",
            )
        } else {
            Error::from(e)
        }
    })
}

fn is_permission_denied(err: &Error) -> bool {
    matches!(err, Error::Message(m) if m.contains("sin permiso") || m.contains("UAC"))
        || matches!(err, Error::Io(e) if e.kind() == std::io::ErrorKind::PermissionDenied)
}

fn hosts_content_equivalent(a: &str, b: &str) -> bool {
    normalize_hosts(a) == normalize_hosts(b)
}

fn normalize_hosts(s: &str) -> String {
    s.replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_string()
}

fn flush_dns_cache() {
    #[cfg(windows)]
    {
        let _ = Command::new("ipconfig")
            .args(["/flushdns"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Escribe el contenido vía UAC sin alterar la lógica de merge (ya viene solo el bloque DevKit).
#[cfg(windows)]
fn write_hosts_elevated(content: &str) -> Result<()> {
    let tmp_dir = std::env::temp_dir();
    let payload = tmp_dir.join("devkit-hosts-payload.txt");
    let script = tmp_dir.join("devkit-hosts-apply.ps1");
    fs::write(&payload, content)?;

    let hosts = hosts_path();
    let payload_s = payload.display().to_string().replace('\'', "''");
    let hosts_s = hosts.display().to_string().replace('\'', "''");
    let ps = format!(
        "$ErrorActionPreference = 'Stop'\r\n\
         Copy-Item -LiteralPath '{payload_s}' -Destination '{hosts_s}' -Force\r\n\
         ipconfig /flushdns | Out-Null\r\n"
    );
    fs::write(&script, ps)?;

    let script_s = script.display().to_string().replace('\'', "''");
    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "Start-Process -FilePath powershell -Verb RunAs -Wait -WindowStyle Hidden \
                 -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File','{script_s}'"
            ),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| Error::msg(format!("no se pudo pedir UAC para hosts: {e}")))?;

    if !status.success() {
        return Err(Error::msg(
            "no se actualizó hosts (UAC cancelado o denegado). Acepta el aviso de administrador; no se borran entradas de Laragon u otras.",
        ));
    }

    // Comprueba que el bloque quedó escrito.
    let written = fs::read_to_string(&hosts).unwrap_or_default();
    if !written.contains(BEGIN) || !written.contains(END) {
        return Err(Error::msg(
            "hosts no contiene el bloque DevKit tras UAC; revisa permisos",
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn write_hosts_elevated(_content: &str) -> Result<()> {
    Err(Error::msg("elevación de hosts solo está implementada en Windows"))
}

/// Vista previa del bloque que DevKit escribiría (sin tocar el disco).
pub fn preview_block(sites: &[Site]) -> String {
    render_block(sites)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Site;
    use std::path::PathBuf;

    fn site(name: &str) -> Site {
        Site {
            name: name.into(),
            hostname: format!("{name}.test"),
            root: PathBuf::from(format!(r"C:\DevKit\www\{name}")),
            php_version: "8.3".into(),
            ssl: false,
            url: String::new(),
        }
    }

    #[test]
    fn replaces_existing_block() {
        let orig = "10.0.1.3 other\r\n# BEGIN DEVKIT\r\n127.0.0.1 old.test\r\n# END DEVKIT\r\n";
        let out = merge_hosts(orig, &[site("blog")]);
        assert!(out.contains("127.0.0.1 blog.test"));
        assert!(!out.contains("old.test"));
        assert!(out.contains("10.0.1.3 other"));
        assert_eq!(out.matches(BEGIN).count(), 1);
    }

    #[test]
    fn preserves_laragon_and_other_entries() {
        let orig = "\
# Copyright\r\n\
127.0.0.1       localhost\r\n\
::1             localhost\r\n\
127.0.0.1      carrito.test         #laragon magic!\r\n\
127.0.0.1      demo.test            #laragon magic!\r\n\
127.0.0.1      www.test             #laragon magic!\r\n\
";
        let out = merge_hosts(orig, &[site("adminer"), site("default")]);
        assert!(out.contains("carrito.test"));
        assert!(out.contains("#laragon magic!"));
        assert!(out.contains("www.test"));
        assert!(out.contains("127.0.0.1 adminer.test"));
        assert!(out.contains("127.0.0.1 default.test"));
        assert!(out.contains("# BEGIN DEVKIT"));
        // Laragon demo.test sigue ahí aunque DevKit también gestione otros sitios.
        assert!(out.contains("demo.test            #laragon magic!"));
    }

    #[test]
    fn updating_devkit_block_does_not_drop_neighbors() {
        let orig = "\
127.0.0.1      carrito.test         #laragon magic!\r\n\
# BEGIN DEVKIT\r\n\
127.0.0.1 adminer.test\r\n\
# END DEVKIT\r\n\
127.0.0.1      keep-me.local\r\n\
";
        let out = merge_hosts(orig, &[site("adminer"), site("default"), site("demo")]);
        assert!(out.contains("carrito.test"));
        assert!(out.contains("keep-me.local"));
        assert!(out.contains("default.test"));
        assert!(out.contains("demo.test"));
        assert_eq!(out.matches(BEGIN).count(), 1);
        assert_eq!(out.matches(END).count(), 1);
    }

    #[test]
    fn appends_block_when_missing() {
        let orig = "127.0.0.1 localhost\r\n";
        let out = merge_hosts(orig, &[site("adminer")]);
        assert!(out.starts_with("127.0.0.1 localhost"));
        assert!(out.contains("# BEGIN DEVKIT"));
        assert!(out.contains("adminer.test"));
    }
}

//! Compila `devkit-mcp` una vez, desde el checkout, sin shell.

use crate::error::{Error, Result};
use crate::win_spawn::StdioInheritGuard;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Serialize)]
pub struct McpExeInfo {
    pub path: String,
    pub exists: bool,
    pub can_build: bool,
}

fn mcp_exe_name() -> &'static str {
    if cfg!(windows) {
        "devkit-mcp.exe"
    } else {
        "devkit-mcp"
    }
}

/// Copia de desarrollo, generada con cargo.
pub fn mcp_release_exe(root: &Path) -> PathBuf {
    root.join("target").join("release").join(mcp_exe_name())
}

/// Orden: junto al panel, `bin/`, y por último `target/release`.
pub fn mcp_candidate_paths(root: &Path) -> Vec<PathBuf> {
    let name = mcp_exe_name();
    let mut paths = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            paths.push(dir.join(name));
        }
    }
    paths.push(root.join("bin").join(name));
    paths.push(mcp_release_exe(root));
    paths
}

pub fn mcp_source_present(root: &Path) -> bool {
    root.join("Cargo.toml").is_file() && root.join("crates").join("devkit-mcp").is_dir()
}

pub fn mcp_exe_info(root: &Path) -> McpExeInfo {
    let candidates = mcp_candidate_paths(root);
    let found = candidates.iter().find(|p| p.is_file());
    let path = found
        .cloned()
        .unwrap_or_else(|| mcp_release_exe(root));
    McpExeInfo {
        exists: found.is_some(),
        can_build: mcp_source_present(root),
        path: path.display().to_string(),
    }
}

/// Genera el exe de release si falta. No recompila si ya está.
pub fn build_mcp_release(root: &Path) -> Result<String> {
    let exe = mcp_release_exe(root);
    if exe.is_file() {
        return Ok(format!("ya existe: {}", exe.display()));
    }
    if !mcp_source_present(root) {
        return Err(Error::msg(
            "no hay código fuente de DevKit en esta carpeta; Generar exe solo funciona desde el repo con cargo",
        ));
    }

    let _guard = StdioInheritGuard::clear();
    let mut cmd = Command::new("cargo");
    cmd.args(["build", "-p", "devkit-mcp", "--release"])
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }

    let output = cmd.output().map_err(|e| {
        Error::msg(format!(
            "no se pudo ejecutar cargo ({e}). Instala Rust y vuelve a intentar."
        ))
    })?;
    if !output.status.success() || !exe.is_file() {
        let err = String::from_utf8_lossy(&output.stderr);
        let tail: Vec<&str> = err.lines().rev().take(12).collect();
        let tail = tail.into_iter().rev().collect::<Vec<_>>().join("\n");
        return Err(Error::msg(format!("cargo no generó el exe.\n{tail}")));
    }
    Ok(format!("generado: {}", exe.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn release_path_uses_target_release() {
        let path = mcp_release_exe(Path::new(r"C:\DevKit"));
        assert!(path.ends_with(Path::new("target").join("release").join(if cfg!(windows) {
            "devkit-mcp.exe"
        } else {
            "devkit-mcp"
        })));
    }

    #[test]
    fn missing_source_does_not_spawn_cargo() {
        let dir = std::env::temp_dir().join(format!("devkit-mcp-nosrc-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let err = build_mcp_release(&dir).unwrap_err().to_string();
        assert!(err.contains("código fuente"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn info_finds_bin_copy_before_release() {
        let dir = std::env::temp_dir().join(format!("devkit-mcp-bin-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let bin = dir.join("bin").join(if cfg!(windows) {
            "devkit-mcp.exe"
        } else {
            "devkit-mcp"
        });
        fs::create_dir_all(bin.parent().unwrap()).unwrap();
        fs::write(&bin, b"stub").unwrap();
        let earlier_exists = mcp_candidate_paths(&dir)
            .into_iter()
            .take_while(|p| p != &bin)
            .any(|p| p.is_file());
        let info = mcp_exe_info(&dir);
        assert!(info.exists, "{}", info.path);
        if !earlier_exists {
            assert_eq!(info.path, bin.display().to_string());
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn existing_exe_skips_build() {
        let dir = std::env::temp_dir().join(format!("devkit-mcp-exists-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let exe = mcp_release_exe(&dir);
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(&exe, b"stub").unwrap();
        let msg = build_mcp_release(&dir).unwrap();
        assert!(msg.contains("ya existe"));
        let _ = fs::remove_dir_all(&dir);
    }
}

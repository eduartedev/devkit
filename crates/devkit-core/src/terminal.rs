use crate::error::{Error, Result};
use crate::path_env;
use crate::paths::{AppConfig, Paths};
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// CREATE_NEW_CONSOLE: una ventana nueva sin pasar por `start` / `wt.exe`.
#[cfg(windows)]
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

pub fn isolated_path(paths: &Paths, cfg: &AppConfig) -> String {
    let prepend = path_env::devkit_path_dirs(paths, cfg);
    let orig = std::env::var("PATH").unwrap_or_default();
    path_env::merge_path_entries(&prepend, &[orig])
}

/// Abre una sola consola `cmd` con PATH de DevKit y cwd en la raíz del kit.
pub fn open_terminal(paths: &Paths, cfg: &AppConfig) -> Result<()> {
    let path = isolated_path(paths, cfg);
    let mut cmd = Command::new("cmd.exe");
    cmd.env("PATH", &path)
        .current_dir(&paths.root)
        .args(["/K", "title DevKit"]);

    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NEW_CONSOLE);
    }

    cmd.spawn()
        .map_err(|e| Error::msg(format!("no se pudo abrir terminal: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;
    use tempfile::tempdir;

    #[test]
    fn isolated_path_puts_php_and_composer_first() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs_layout(root);
        let paths = Paths::from_root(root.to_path_buf());
        let cfg = AppConfig {
            php_version: "8.3".into(),
            ..AppConfig::default()
        };
        let path = isolated_path(&paths, &cfg);
        let php = paths.php_dir("8.3").to_string_lossy().into_owned();
        let composer = paths.bin.join("composer").to_string_lossy().into_owned();
        assert!(
            path.starts_with(&format!("{php};{composer};")),
            "PATH debe priorizar PHP luego Composer: {path}"
        );
    }

    #[test]
    fn isolated_path_includes_node_and_mariadb_when_present() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs_layout(root);
        std::fs::create_dir_all(root.join("bin/node")).unwrap();
        std::fs::write(root.join("bin/node/node.exe"), []).unwrap();
        std::fs::create_dir_all(root.join("bin/mariadb/bin")).unwrap();
        let paths = Paths::from_root(root.to_path_buf());
        let cfg = AppConfig {
            php_version: "8.3".into(),
            ..AppConfig::default()
        };
        let path = isolated_path(&paths, &cfg);
        assert!(path.contains("node"));
        assert!(path.contains("mariadb"));
    }

    fn fs_layout(root: &std::path::Path) {
        std::fs::create_dir_all(root.join("bin/php/8.3")).unwrap();
        std::fs::create_dir_all(root.join("bin/composer")).unwrap();
        std::fs::create_dir_all(root.join("www")).unwrap();
        std::fs::create_dir_all(root.join("etc/devkit")).unwrap();
        std::fs::create_dir_all(root.join("logs")).unwrap();
        std::fs::create_dir_all(root.join("tmp")).unwrap();
        std::fs::create_dir_all(root.join("data")).unwrap();
    }
}

use crate::error::{Error, Result};
use crate::paths::{AppConfig, PathEnvScope, Paths};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Carpetas de `bin/` que DevKit antepone al PATH (mismo orden que la terminal).
pub fn devkit_path_dirs(paths: &Paths, cfg: &AppConfig) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    push_dir(&mut out, paths.php_dir(&cfg.php_version));
    if paths.node_exe().is_file() {
        push_dir(&mut out, paths.bin.join("node"));
    }
    let maria = paths.bin.join("mariadb").join("bin");
    if maria.is_dir() {
        push_dir(&mut out, maria);
    }
    if paths.git_exe().is_file() {
        if let Some(cmd) = paths.git_exe().parent() {
            push_dir(&mut out, cmd.to_path_buf());
        }
    }
    push_dir(&mut out, paths.bin.join("composer"));
    out
}

fn push_dir(out: &mut Vec<String>, dir: PathBuf) {
    out.push(dir.to_string_lossy().into_owned());
}

/// Quita entradas bajo `{root}/bin` (cualquier instalación previa de este DevKit).
pub fn strip_devkit_bin_paths(existing: &str, root: &Path) -> Vec<String> {
    existing
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter(|s| !entry_under_devkit_bin(s, root))
        .map(|s| s.to_string())
        .collect()
}

fn entry_under_devkit_bin(entry: &str, root: &Path) -> bool {
    let entry_path = PathBuf::from(entry);
    let canon = entry_path.canonicalize().unwrap_or(entry_path);
    let bin = root.join("bin");
    let bin_canon = bin.canonicalize().unwrap_or(bin);
    canon.starts_with(&bin_canon)
}

/// Antepone `prepend` sin duplicar (comparación case-insensitive en Windows).
pub fn merge_path_entries(prepend: &[String], rest: &[String]) -> String {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for s in prepend.iter().chain(rest.iter()) {
        let key = s.to_ascii_lowercase();
        if seen.insert(key) {
            out.push(s.clone());
        }
    }
    out.join(";")
}

pub fn apply(paths: &Paths, cfg: &AppConfig, scope: PathEnvScope) -> Result<Vec<String>> {
    let scope = active_scope(scope)?;
    let prepend = devkit_path_dirs(paths, cfg);
    let current = read_path(scope)?;
    let rest = strip_devkit_bin_paths(&current, &paths.root);
    let merged = merge_path_entries(&prepend, &rest);
    write_path(scope, &merged)?;
    Ok(prepend)
}

pub fn remove(paths: &Paths, scope: PathEnvScope) -> Result<()> {
    let scope = active_scope(scope)?;
    let current = read_path(scope)?;
    let rest = strip_devkit_bin_paths(&current, &paths.root);
    let merged = merge_path_entries(&[], &rest);
    write_path(scope, &merged)?;
    Ok(())
}

pub fn remove_all(paths: &Paths) -> Result<()> {
    let user_ok = remove(paths, PathEnvScope::User).is_ok();
    let machine_ok = remove(paths, PathEnvScope::Machine).is_ok();
    if user_ok || machine_ok {
        Ok(())
    } else {
        remove(paths, PathEnvScope::User)
    }
}

pub fn sync_configured(paths: &Paths, cfg: &AppConfig) -> Result<()> {
    match cfg.path_scope {
        PathEnvScope::Off => Ok(()),
        scope => {
            apply(paths, cfg, scope)?;
            Ok(())
        }
    }
}

fn active_scope(scope: PathEnvScope) -> Result<PathEnvScope> {
    match scope {
        PathEnvScope::Off => Err(Error::msg("path_scope inválido para aplicar PATH")),
        PathEnvScope::User | PathEnvScope::Machine => Ok(scope),
    }
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, REG_SZ, REG_VALUE_TYPE,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };

    const MACHINE_ENV: &str =
        "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment";

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(Some(0)).collect()
    }

    fn hive(scope: PathEnvScope) -> (HKEY, &'static str) {
        match scope {
            PathEnvScope::User => (HKEY_CURRENT_USER, "Environment"),
            PathEnvScope::Machine => (HKEY_LOCAL_MACHINE, MACHINE_ENV),
            PathEnvScope::Off => unreachable!(),
        }
    }

    pub(super) fn read_path(scope: PathEnvScope) -> Result<String> {
        unsafe {
            let (hive, subkey) = hive(scope);
            let sub = wide(subkey);
            let mut key = HKEY::default();
            RegOpenKeyExW(hive, PCWSTR(sub.as_ptr()), 0, KEY_READ, &mut key)
                .ok()
                .map_err(|e| registry_err("abrir clave PATH", scope, e))?;
            let path = query_path_value(&key)?;
            let _ = RegCloseKey(key);
            Ok(path)
        }
    }

    pub(super) fn write_path(scope: PathEnvScope, value: &str) -> Result<()> {
        unsafe {
            let (hive, subkey) = hive(scope);
            let sub = wide(subkey);
            let mut key = HKEY::default();
            RegOpenKeyExW(hive, PCWSTR(sub.as_ptr()), 0, KEY_SET_VALUE, &mut key)
                .ok()
                .map_err(|e| {
                    registry_err(
                        "escribir PATH (¿administrador para todos los usuarios?)",
                        scope,
                        e,
                    )
                })?;
            let name = wide("Path");
            let data = wide(value);
            let bytes = std::slice::from_raw_parts(
                data.as_ptr() as *const u8,
                data.len() * 2,
            );
            RegSetValueExW(key, PCWSTR(name.as_ptr()), 0, REG_SZ, Some(bytes))
                .ok()
                .map_err(|e| {
                    let _ = RegCloseKey(key);
                    registry_err("guardar PATH", scope, e)
                })?;
            let _ = RegCloseKey(key);
            notify_environment_changed();
            Ok(())
        }
    }

    unsafe fn query_path_value(key: &HKEY) -> Result<String> {
        let name = wide("Path");
        let mut ty = REG_VALUE_TYPE::default();
        let mut size = 0u32;
        let _ = RegQueryValueExW(
            *key,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut ty as *mut _),
            None,
            Some(&mut size),
        );
        if size == 0 {
            return Ok(String::new());
        }
        let mut buf: Vec<u8> = vec![0; size as usize];
        RegQueryValueExW(
            *key,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut ty as *mut _),
            Some(buf.as_mut_ptr()),
            Some(&mut size),
        )
        .ok()
        .map_err(|e| Error::msg(format!("leer Path del registro: {e}")))?;
        let wide_len = size as usize / 2;
        let u16s: &[u16] =
            std::slice::from_raw_parts(buf.as_ptr() as *const u16, wide_len);
        let end = u16s.iter().position(|&c| c == 0).unwrap_or(u16s.len());
        Ok(String::from_utf16_lossy(&u16s[..end]))
    }

    fn notify_environment_changed() {
        let env = wide("Environment");
        unsafe {
            let _ = SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM(0),
                LPARAM(env.as_ptr() as isize),
                SMTO_ABORTIFHUNG,
                5000,
                None,
            );
        }
    }

    fn registry_err(action: &str, scope: PathEnvScope, e: windows::core::Error) -> Error {
        let who = match scope {
            PathEnvScope::User => "usuario actual",
            PathEnvScope::Machine => "todos los usuarios",
            PathEnvScope::Off => "",
        };
        Error::msg(format!("no se pudo {action} ({who}): {e}"))
    }
}

#[cfg(windows)]
use win::{read_path, write_path};

#[cfg(not(windows))]
fn read_path(_scope: PathEnvScope) -> Result<String> {
    Err(Error::msg("PATH del sistema solo está soportado en Windows"))
}

#[cfg(not(windows))]
fn write_path(_scope: PathEnvScope, _value: &str) -> Result<()> {
    Err(Error::msg("PATH del sistema solo está soportado en Windows"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;
    use tempfile::tempdir;

    #[test]
    fn strip_removes_only_bin_under_root() {
        let root = PathBuf::from(r"C:\DevKit");
        let existing = format!(
            r"C:\Other\bin;{}\bin\php\8.3;C:\Windows\System32",
            root.display()
        );
        let rest = strip_devkit_bin_paths(&existing, &root);
        assert_eq!(rest, ["C:\\Other\\bin", "C:\\Windows\\System32"]);
    }

    #[test]
    fn merge_dedupes_case_insensitive() {
        let merged = merge_path_entries(
            &[r"C:\DevKit\bin\php\8.3".into()],
            &[r"c:\devkit\bin\php\8.3".into(), "C:\\Windows".into()],
        );
        assert_eq!(merged, r"C:\DevKit\bin\php\8.3;C:\Windows");
    }

    #[test]
    fn devkit_path_dirs_order_and_git_cmd() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("bin/php/8.4")).unwrap();
        std::fs::create_dir_all(root.join("bin/node")).unwrap();
        std::fs::write(root.join("bin/node/node.exe"), []).unwrap();
        std::fs::create_dir_all(root.join("bin/mariadb/bin")).unwrap();
        std::fs::create_dir_all(root.join("bin/git/cmd")).unwrap();
        std::fs::write(root.join("bin/git/cmd/git.exe"), []).unwrap();
        std::fs::create_dir_all(root.join("bin/composer")).unwrap();
        let paths = Paths::from_root(root.to_path_buf());
        let cfg = AppConfig {
            php_version: "8.4".into(),
            ..AppConfig::default()
        };
        let dirs = devkit_path_dirs(&paths, &cfg);
        assert!(dirs[0].ends_with(r"bin\php\8.4") || dirs[0].ends_with("bin/php/8.4"));
        assert!(dirs.iter().any(|d| d.contains("mariadb")));
        assert!(dirs.iter().any(|d| d.contains("git") && d.contains("cmd")));
        assert!(
            dirs.last()
                .map(|d| d.ends_with("composer") || d.ends_with("composer\\"))
                .unwrap_or(false)
        );
    }
}

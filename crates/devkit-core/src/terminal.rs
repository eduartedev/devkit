use crate::error::{Error, Result};
use crate::paths::{AppConfig, Paths};
use std::process::Command;

pub fn isolated_path(paths: &Paths, cfg: &AppConfig) -> String {
    let mut parts: Vec<String> = Vec::new();
    let php = paths.php_dir(&cfg.php_version);
    parts.push(php.to_string_lossy().into_owned());
    let node = paths.bin.join("node");
    if node.join("node.exe").exists() {
        parts.push(node.to_string_lossy().into_owned());
    }
    let maria_bin = paths.bin.join("mariadb").join("bin");
    if maria_bin.exists() {
        parts.push(maria_bin.to_string_lossy().into_owned());
    }
    let composer_dir = paths.bin.join("composer");
    parts.push(composer_dir.to_string_lossy().into_owned());
    let orig = std::env::var("PATH").unwrap_or_default();
    format!("{};{}", parts.join(";"), orig)
}

pub fn open_terminal(paths: &Paths, cfg: &AppConfig) -> Result<()> {
    let path = isolated_path(paths, cfg);
    let wt = r"C:\Users\{}\AppData\Local\Microsoft\WindowsApps\wt.exe";
    let user = std::env::var("USERNAME").unwrap_or_default();
    let wt_path = wt.replace("{}", &user);
    let title = "DevKit";
    let _cmd_line = format!(
        "cmd.exe /K \"set PATH={path}&& title {title}&& cd /d {}\"",
        paths.root.display()
    );
    if PathExists(&wt_path) || which("wt.exe") {
        let status = Command::new("wt.exe")
            .args(["-d", &paths.root.to_string_lossy(), "cmd", "/K", &format!("set PATH={path}&& title DevKit")])
            .spawn();
        if status.is_ok() {
            return Ok(());
        }
    }
    Command::new("cmd.exe")
        .args(["/C", "start", "DevKit", "cmd.exe", "/K", &format!("set PATH={path}&& cd /d {}", paths.root.display())])
        .spawn()
        .map_err(|e| Error::msg(format!("no se pudo abrir terminal: {e}")))?;
    Ok(())
}

fn which(name: &str) -> bool {
    Command::new("where")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[allow(non_snake_case)]
fn PathExists(p: &str) -> bool {
    std::path::Path::new(p).is_file()
}

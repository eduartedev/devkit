use crate::error::Result;
use crate::paths::Paths;
use rand::Rng;
use std::fs;
use std::path::Path;
use std::process::Command;

pub fn random_secret(len: usize) -> String {
    const ALPH: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| ALPH[rng.gen_range(0..ALPH.len())] as char)
        .collect()
}

pub fn write_secret_file(path: &Path, value: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, value.trim())?;
    restrict_acl(path);
    Ok(())
}

pub fn read_or_create_secret(path: &Path, len: usize) -> Result<String> {
    if path.exists() {
        let v = fs::read_to_string(path)?;
        let t = v.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    let secret = random_secret(len);
    write_secret_file(path, &secret)?;
    Ok(secret)
}

fn restrict_acl(path: &Path) {
    #[cfg(windows)]
    {
        let user = std::env::var("USERNAME").unwrap_or_else(|_| "Users".into());
        let _ = Command::new("icacls")
            .arg(path)
            .args(["/inheritance:r", "/grant:r", &format!("{user}:(R,W)")])
            .output();
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(path) {
            let mut p = meta.permissions();
            p.set_mode(0o600);
            let _ = fs::set_permissions(path, p);
        }
    }
}

pub fn mysql_password(paths: &Paths) -> Result<String> {
    read_or_create_secret(&paths.mysql_password_file(), 24)
}

pub fn redis_password(paths: &Paths) -> Result<String> {
    read_or_create_secret(&paths.redis_password_file(), 24)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn secret_persists() {
        let d = tempdir().unwrap();
        let p = d.path().join("s.txt");
        let a = read_or_create_secret(&p, 16).unwrap();
        let b = read_or_create_secret(&p, 16).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);
    }
}

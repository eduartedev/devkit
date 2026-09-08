use crate::error::{Error, Result};
use crate::paths::Paths;
use rand::Rng;
use serde::{Deserialize, Serialize};
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
    fs::write(path, value)?;
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

/// Lee un secreto; si el archivo no existe, lo crea con `default` (puede ser vacío).
pub fn read_or_create_default(path: &Path, default: &str) -> Result<String> {
    if path.exists() {
        let v = fs::read_to_string(path)?;
        return Ok(v.trim_end_matches(['\r', '\n']).to_string());
    }
    write_secret_file(path, default)?;
    Ok(default.to_string())
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbCredentials {
    pub user: String,
    /// Puede ser vacío (default Laragon-like).
    pub password: String,
}

impl DbCredentials {
    pub fn validate(&self) -> Result<()> {
        let user = self.user.trim();
        if user.is_empty() {
            return Err(Error::msg("usuario de MariaDB vacío"));
        }
        if user.len() > 32 {
            return Err(Error::msg("usuario de MariaDB demasiado largo (máx. 32)"));
        }
        if !user
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(Error::msg(
                "usuario inválido: solo letras, números, _ y -",
            ));
        }
        if self.password.len() > 128 {
            return Err(Error::msg("contraseña demasiado larga (máx. 128)"));
        }
        if self.password.chars().any(|c| c == '\0' || c == '\n' || c == '\r') {
            return Err(Error::msg("contraseña contiene caracteres no permitidos"));
        }
        Ok(())
    }

    pub fn normalized(mut self) -> Self {
        self.user = self.user.trim().to_string();
        self
    }
}

/// Por defecto: user=`root`, password vacío (sin pedir login en local).
pub fn mysql_credentials(paths: &Paths) -> Result<DbCredentials> {
    let user = read_or_create_default(&paths.mysql_user_file(), "root")?;
    let password = read_or_create_default(&paths.mysql_password_file(), "")?;
    let creds = DbCredentials { user, password }.normalized();
    creds.validate()?;
    Ok(creds)
}

pub fn write_mysql_credentials(paths: &Paths, creds: &DbCredentials) -> Result<()> {
    let creds = creds.clone().normalized();
    creds.validate()?;
    write_secret_file(&paths.mysql_user_file(), &creds.user)?;
    write_secret_file(&paths.mysql_password_file(), &creds.password)?;
    Ok(())
}

pub fn mysql_password(paths: &Paths) -> Result<String> {
    Ok(mysql_credentials(paths)?.password)
}

pub fn mysql_user(paths: &Paths) -> Result<String> {
    Ok(mysql_credentials(paths)?.user)
}

pub fn redis_password(paths: &Paths) -> Result<String> {
    read_or_create_secret(&paths.redis_password_file(), 24)
}

/// Argumentos `-u` / `-p…` para mysqladmin/mysql (sin `-p` si la clave está vacía).
pub fn mysql_cli_auth_args(user: &str, password: &str) -> Vec<String> {
    let mut args = vec!["-u".into(), user.to_string()];
    if !password.is_empty() {
        args.push(format!("-p{password}"));
    }
    args
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

    #[test]
    fn mysql_defaults_to_root_with_empty_password() {
        let d = tempdir().unwrap();
        let paths = Paths::from_root(d.path().to_path_buf());
        fs::create_dir_all(paths.secrets_dir()).unwrap();
        let c = mysql_credentials(&paths).unwrap();
        assert_eq!(c.user, "root");
        assert_eq!(c.password, "");
        assert!(paths.mysql_password_file().exists());
        assert!(paths.mysql_user_file().exists());
    }

    #[test]
    fn mysql_credentials_roundtrip() {
        let d = tempdir().unwrap();
        let paths = Paths::from_root(d.path().to_path_buf());
        let creds = DbCredentials {
            user: "devkit".into(),
            password: "s3cret".into(),
        };
        write_mysql_credentials(&paths, &creds).unwrap();
        let loaded = mysql_credentials(&paths).unwrap();
        assert_eq!(loaded, creds);
    }

    #[test]
    fn rejects_bad_username() {
        let bad = DbCredentials {
            user: "root;drop".into(),
            password: "".into(),
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn empty_password_omits_dash_p() {
        let args = mysql_cli_auth_args("root", "");
        assert_eq!(args, vec!["-u", "root"]);
        let args = mysql_cli_auth_args("root", "x");
        assert_eq!(
            args,
            vec![
                "-u".to_string(),
                "root".to_string(),
                "-px".to_string()
            ]
        );
    }

    #[test]
    fn keeps_existing_non_empty_password_file() {
        let d = tempdir().unwrap();
        let paths = Paths::from_root(d.path().to_path_buf());
        fs::create_dir_all(paths.secrets_dir()).unwrap();
        fs::write(paths.mysql_password_file(), "already-set\n").unwrap();
        let c = mysql_credentials(&paths).unwrap();
        assert_eq!(c.password, "already-set");
        assert_eq!(c.user, "root");
    }
}

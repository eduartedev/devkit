use crate::error::{Error, Result};
use crate::paths::Paths;
use crate::process_stream::{run_logged, write_log_line};
use crate::site::{parse_site_name, site_root};
use std::fs;
use std::path::{Component, Path};
use std::process::Command;

pub fn create_blank(paths: &Paths, name: &str) -> Result<std::path::PathBuf> {
    let safe = parse_site_name(name)?;
    let root = site_root(paths, &safe)?;
    if root.exists() {
        return Err(Error::msg(format!("ya existe {}", root.display())));
    }
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("index.php"),
        format!(
            "<?php\ndeclare(strict_types=1);\necho '<h1>{}</h1>';\n",
            html_escape(&safe)
        ),
    )?;
    Ok(root)
}

pub fn create_laravel(
    paths: &Paths,
    php_version: &str,
    name: &str,
    on_line: &mut impl FnMut(&str),
) -> Result<std::path::PathBuf> {
    let safe = parse_site_name(name)?;
    let root = site_root(paths, &safe)?;
    if root.exists() {
        return Err(Error::msg(format!("ya existe {}", root.display())));
    }
    let php = paths.php_exe(php_version);
    let composer = paths.composer();
    if !php.exists() {
        return Err(Error::MissingBinary { path: php });
    }
    if !composer.exists() {
        return Err(Error::MissingBinary { path: composer });
    }
    write_log_line(
        on_line,
        &format!(
            "→ composer create-project laravel/laravel {safe} (--prefer-dist --no-interaction)"
        ),
    );
    let mut cmd = Command::new(&php);
    cmd.arg(&composer).args([
        "create-project",
        "--prefer-dist",
        "--no-interaction",
        "laravel/laravel",
        &safe,
    ]);
    cmd.current_dir(&paths.www);
    let status = run_logged(&mut cmd, on_line)?;
    if !status.success() {
        return Err(Error::msg("composer create-project laravel falló"));
    }
    // Laravel sirve desde public/; nginx lo detecta al regenerar vhosts.
    let artisan = root.join("artisan");
    if artisan.is_file() {
        write_log_line(on_line, "→ php artisan migrate --force --no-interaction");
        let mut migrate = Command::new(&php);
        migrate
            .arg(&artisan)
            .args(["migrate", "--force", "--no-interaction"])
            .current_dir(&root);
        let _ = run_logged(&mut migrate, on_line);
    }
    Ok(root)
}

pub fn create_wordpress(paths: &Paths, name: &str) -> Result<std::path::PathBuf> {
    let safe = parse_site_name(name)?;
    let root = site_root(paths, &safe)?;
    if root.exists() {
        return Err(Error::msg(format!("ya existe {}", root.display())));
    }
    let zip_path = paths.tmp.join("wordpress-latest.zip");
    crate::download::download_verified("https://wordpress.org/latest.zip", &zip_path, None)?;
    let dest_tmp = paths.tmp.join(format!("wp-{safe}"));
    if dest_tmp.exists() {
        fs::remove_dir_all(&dest_tmp)?;
    }
    crate::download::unzip_safe(&zip_path, &dest_tmp)?;
    let inner = dest_tmp.join("wordpress");
    if inner.is_dir() {
        fs::rename(inner, &root)?;
        let _ = fs::remove_dir_all(dest_tmp);
    } else {
        fs::rename(dest_tmp, &root)?;
    }
    Ok(root)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Borra `www/{name}`. Junctions/symlinks solo se desvinculan (no tocan el destino).
pub fn delete_site(paths: &Paths, name: &str) -> Result<()> {
    let safe = parse_site_name(name)?;
    if safe == "default" {
        return Err(Error::msg("no se puede eliminar el sitio 'default'"));
    }
    let root = paths.www.join(&safe);
    if root.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(Error::msg("path traversal"));
    }
    if root.file_name().and_then(|s| s.to_str()) != Some(safe.as_str()) {
        return Err(Error::msg("nombre de sitio inválido"));
    }

    let meta = match fs::symlink_metadata(&root) {
        Ok(m) => m,
        Err(_) => {
            return Err(Error::msg(format!("no existe {}", root.display())));
        }
    };
    let ft = meta.file_type();
    if ft.is_symlink() {
        // No seguir el enlace: solo quitar la entrada en www/.
        unlink_dir_link(&root)?;
    } else if ft.is_dir() {
        crate::bind::ensure_under(&paths.www, &root)?;
        fs::remove_dir_all(&root)?;
    } else {
        return Err(Error::msg(format!(
            "{} no es una carpeta de sitio",
            root.display()
        )));
    }
    Ok(())
}

fn unlink_dir_link(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        fs::remove_dir(path).map_err(Error::from)
    }
    #[cfg(not(windows))]
    {
        fs::remove_file(path).map_err(Error::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;

    #[test]
    fn deletes_plain_site_dir() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::from_root(dir.path().to_path_buf());
        fs::create_dir_all(paths.www.join("blog")).unwrap();
        fs::write(paths.www.join("blog").join("index.php"), "<?php").unwrap();
        delete_site(&paths, "blog").unwrap();
        assert!(!paths.www.join("blog").exists());
    }

    #[test]
    fn rejects_default() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::from_root(dir.path().to_path_buf());
        fs::create_dir_all(paths.www.join("default")).unwrap();
        assert!(delete_site(&paths, "default").is_err());
        assert!(paths.www.join("default").exists());
    }

    #[test]
    fn unlinks_junction_without_wiping_target() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::from_root(dir.path().to_path_buf());
        let real = dir.path().join("real-app");
        fs::create_dir_all(&paths.www).unwrap();
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join("keep.txt"), "ok").unwrap();
        let link = paths.www.join("linked");
        #[cfg(windows)]
        {
            let status = std::process::Command::new("cmd")
                .args([
                    "/C",
                    "mklink",
                    "/J",
                    &link.to_string_lossy(),
                    &real.to_string_lossy(),
                ])
                .status()
                .unwrap();
            assert!(status.success());
        }
        #[cfg(not(windows))]
        {
            std::os::unix::fs::symlink(&real, &link).unwrap();
        }
        delete_site(&paths, "linked").unwrap();
        assert!(!link.exists());
        assert!(real.join("keep.txt").is_file(), "el destino real debe quedar");
    }
}

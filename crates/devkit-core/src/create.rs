use crate::error::{Error, Result};
use crate::paths::Paths;
use crate::site::{parse_site_name, site_root};
use std::fs;
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

pub fn create_laravel(paths: &Paths, php_version: &str, name: &str) -> Result<std::path::PathBuf> {
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
    let status = Command::new(&php)
        .arg(&composer)
        .args([
            "create-project",
            "--prefer-dist",
            "laravel/laravel",
            &safe,
        ])
        .current_dir(&paths.www)
        .status()?;
    if !status.success() {
        return Err(Error::msg("composer create-project laravel falló"));
    }
    // Laravel sirve desde public/; nginx lo detecta al regenerar vhosts.
    let artisan = root.join("artisan");
    let _ = Command::new(&php)
        .arg(&artisan)
        .args(["migrate", "--force"])
        .current_dir(&root)
        .status();
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

use crate::error::{Error, Result};
use crate::model::Site;
use crate::paths::{AppConfig, Paths};
use std::process::Command;

pub fn issue_certs(paths: &Paths, cfg: &mut AppConfig, sites: &[Site]) -> Result<String> {
    let mk = paths.mkcert();
    if !mk.exists() {
        return Err(Error::MissingBinary { path: mk });
    }
    let mut names = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        format!("*.{}", cfg.tld),
    ];
    for s in sites {
        names.push(s.hostname.clone());
    }
    names.sort();
    names.dedup();

    let mut install = Command::new(&mk);
    install.arg("-install");
    let _ = crate::win_spawn::status_silent(&mut install);

    let mut issue = Command::new(&mk);
    issue
        .arg("-cert-file")
        .arg(paths.ssl_cert())
        .arg("-key-file")
        .arg(paths.ssl_key())
        .args(&names);
    let status = crate::win_spawn::status_silent(&mut issue)?;
    if !status.success() {
        return Err(Error::msg(
            "mkcert falló (si pide admin, ejecuta `devkit ssl` elevado una vez)",
        ));
    }
    cfg.ssl = true;
    cfg.save(&paths.config_file())?;
    Ok(format!(
        "certificado en {} — HTTPS en {}:{}",
        paths.ssl_cert().display(),
        cfg.bind,
        cfg.https_port
    ))
}

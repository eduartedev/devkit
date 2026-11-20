//! Cron de scripts por sitio vía Programador de tareas de Windows.

use crate::error::{Error, Result};
use crate::path_env::devkit_path_dirs;
use crate::paths::{AppConfig, Paths};
use crate::process_stream::{run_logged, write_log_line};
use crate::site::{parse_site_name, site_root};
use crate::win_spawn;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const MARKER_FILE: &str = ".devkit-cron.toml";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CronEvery {
    Daily,
    Hourly,
    Weekly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CronJob {
    pub id: String,
    pub every: CronEvery,
    /// Hora `HH:MM` (solo daily/weekly). Default 03:00.
    #[serde(default = "default_at")]
    pub at: String,
    /// Argv relativo al sitio. El primer token debe ser `php`|`python`|`node` o un script bajo el sitio.
    pub argv: Vec<String>,
}

fn default_at() -> String {
    "03:00".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CronManifest {
    #[serde(default)]
    pub jobs: Vec<CronJob>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJobInfo {
    pub site: String,
    pub id: String,
    pub every: String,
    pub at: String,
    pub argv: Vec<String>,
    pub task_name: String,
    pub registered: bool,
}

pub fn marker_path(project: &Path) -> PathBuf {
    project.join(MARKER_FILE)
}

pub fn load_manifest(project: &Path) -> Result<CronManifest> {
    let p = marker_path(project);
    if !p.is_file() {
        return Ok(CronManifest::default());
    }
    let text = fs::read_to_string(&p)?;
    let man: CronManifest = toml::from_str(&text)?;
    validate_manifest(&man)?;
    Ok(man)
}

pub fn validate_manifest(man: &CronManifest) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for job in &man.jobs {
        validate_job(job)?;
        if !seen.insert(job.id.clone()) {
            return Err(Error::msg(format!("cron id duplicado '{}'", job.id)));
        }
    }
    Ok(())
}

pub fn validate_job(job: &CronJob) -> Result<()> {
    parse_job_id(&job.id)?;
    if job.argv.is_empty() {
        return Err(Error::msg(format!("cron '{}': argv vacío", job.id)));
    }
    if job.argv.len() > 32 {
        return Err(Error::msg(format!("cron '{}': demasiados args", job.id)));
    }
    for arg in &job.argv {
        validate_arg(arg)?;
    }
    match job.every {
        CronEvery::Hourly => {}
        CronEvery::Daily | CronEvery::Weekly => {
            parse_hhmm(&job.at)?;
        }
    }
    Ok(())
}

fn parse_job_id(raw: &str) -> Result<String> {
    let id = raw.trim();
    if id.is_empty() || id.len() > 48 {
        return Err(Error::msg("cron id inválido"));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(Error::msg(format!(
            "cron id '{id}': solo a-z, 0-9, guion y guion bajo"
        )));
    }
    Ok(id.to_string())
}

fn validate_arg(arg: &str) -> Result<()> {
    if arg.is_empty() {
        return Err(Error::msg("arg vacío en cron"));
    }
    if arg.len() > 256 {
        return Err(Error::msg("arg demasiado largo en cron"));
    }
    if arg.contains("..")
        || arg.contains(['|', '&', ';', '<', '>', '`', '\n', '\r', '\0', '"', '\''])
    {
        return Err(Error::msg(format!("arg de cron inseguro: {arg}")));
    }
    if Path::new(arg).is_absolute() {
        return Err(Error::msg(format!(
            "arg de cron no puede ser ruta absoluta: {arg}"
        )));
    }
    Ok(())
}

fn parse_hhmm(raw: &str) -> Result<(u32, u32)> {
    let parts: Vec<_> = raw.trim().split(':').collect();
    if parts.len() != 2 {
        return Err(Error::msg(format!("hora inválida '{raw}' (usa HH:MM)")));
    }
    let h: u32 = parts[0]
        .parse()
        .map_err(|_| Error::msg(format!("hora inválida '{raw}'")))?;
    let m: u32 = parts[1]
        .parse()
        .map_err(|_| Error::msg(format!("minuto inválido '{raw}'")))?;
    if h > 23 || m > 59 {
        return Err(Error::msg(format!("hora fuera de rango '{raw}'")));
    }
    Ok((h, m))
}

pub fn task_name(site: &str, job_id: &str) -> String {
    format!(r"DevKit\{site}\{job_id}")
}

pub fn list_site_jobs(paths: &Paths, site: &str) -> Result<Vec<CronJobInfo>> {
    let safe = parse_site_name(site)?;
    let root = site_root(paths, &safe)?;
    if !root.is_dir() {
        return Err(Error::msg(format!("no existe el sitio '{safe}'")));
    }
    let man = load_manifest(&root)?;
    let mut out = Vec::new();
    for job in man.jobs {
        let tn = task_name(&safe, &job.id);
        out.push(CronJobInfo {
            site: safe.clone(),
            id: job.id.clone(),
            every: match job.every {
                CronEvery::Daily => "daily".into(),
                CronEvery::Hourly => "hourly".into(),
                CronEvery::Weekly => "weekly".into(),
            },
            at: job.at.clone(),
            argv: job.argv.clone(),
            task_name: tn.clone(),
            registered: task_exists(&tn),
        });
    }
    Ok(out)
}

pub fn list_all_jobs(paths: &Paths) -> Result<Vec<CronJobInfo>> {
    let mut out = Vec::new();
    if !paths.www.is_dir() {
        return Ok(out);
    }
    for entry in fs::read_dir(&paths.www)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let Ok(safe) = parse_site_name(&name) else {
            continue;
        };
        if !marker_path(&entry.path()).is_file() {
            continue;
        }
        out.extend(list_site_jobs(paths, &safe)?);
    }
    out.sort_by(|a, b| (&a.site, &a.id).cmp(&(&b.site, &b.id)));
    Ok(out)
}

pub fn sync_site(paths: &Paths, cfg: &AppConfig, site: &str) -> Result<Vec<String>> {
    let safe = parse_site_name(site)?;
    let root = site_root(paths, &safe)?;
    let man = load_manifest(&root)?;
    let exe = devkit_exe()?;
    let mut report = Vec::new();
    for job in &man.jobs {
        let tn = task_name(&safe, &job.id);
        let tr = format!(
            "\"{}\" cron run {} {}",
            exe.display(),
            safe,
            job.id
        );
        create_or_update_task(&tn, &tr, job)?;
        report.push(format!("sync {tn}"));
    }
    let _ = cfg; // PATH se aplica en `run_job`
    Ok(report)
}

pub fn sync_all(paths: &Paths, cfg: &AppConfig) -> Result<Vec<String>> {
    let mut report = Vec::new();
    for info in list_all_jobs(paths)? {
        report.extend(sync_site(paths, cfg, &info.site)?);
    }
    // Dedup sites already synced
    report.sort();
    report.dedup();
    // Re-sync properly per unique site
    let mut sites: Vec<String> = list_all_jobs(paths)?
        .into_iter()
        .map(|j| j.site)
        .collect();
    sites.sort();
    sites.dedup();
    report.clear();
    for site in sites {
        report.extend(sync_site(paths, cfg, &site)?);
    }
    Ok(report)
}

pub fn disable_site(paths: &Paths, site: &str) -> Result<Vec<String>> {
    let jobs = list_site_jobs(paths, site)?;
    let mut report = Vec::new();
    for job in jobs {
        if delete_task(&job.task_name)? {
            report.push(format!("disable {}", job.task_name));
        }
    }
    Ok(report)
}

pub fn disable_all(paths: &Paths) -> Result<Vec<String>> {
    let mut report = Vec::new();
    let mut sites: Vec<String> = list_all_jobs(paths)?
        .into_iter()
        .map(|j| j.site)
        .collect();
    sites.sort();
    sites.dedup();
    for site in sites {
        report.extend(disable_site(paths, &site)?);
    }
    Ok(report)
}

pub fn run_job(
    paths: &Paths,
    cfg: &AppConfig,
    site: &str,
    job_id: &str,
    on_line: &mut impl FnMut(&str),
) -> Result<()> {
    let safe = parse_site_name(site)?;
    let jid = parse_job_id(job_id)?;
    let root = site_root(paths, &safe)?;
    let man = load_manifest(&root)?;
    let job = man
        .jobs
        .iter()
        .find(|j| j.id == jid)
        .ok_or_else(|| Error::msg(format!("cron '{jid}' no está en {safe}")))?;
    validate_job(job)?;

    let (program, args) = resolve_command(paths, cfg, &root, &job.argv)?;
    let log_path = paths.logs.join(format!("cron-{safe}-{jid}.log"));
    write_log_line(
        on_line,
        &format!("→ {} {}", program.display(), args.join(" ")),
    );

    let mut cmd = Command::new(&program);
    cmd.args(&args)
        .current_dir(&root)
        .env("PATH", cron_path(paths, cfg))
        .env("DEVKIT_ROOT", paths.root.display().to_string())
        .env("DEVKIT_SITE", &safe);
    let status = run_logged(&mut cmd, on_line)?;
    let line = format!(
        "cron {safe}/{jid} exit {}\n",
        status.code().unwrap_or(-1)
    );
    let _ = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .and_then(|mut f| {
            use std::io::Write;
            f.write_all(line.as_bytes())
        });
    if !status.success() {
        return Err(Error::msg(format!(
            "cron {safe}/{jid} falló (código {})",
            status.code().unwrap_or(-1)
        )));
    }
    Ok(())
}

fn resolve_command(
    paths: &Paths,
    cfg: &AppConfig,
    root: &Path,
    argv: &[String],
) -> Result<(PathBuf, Vec<String>)> {
    let head = argv[0].as_str();
    let rest: Vec<String> = argv[1..].to_vec();
    match head {
        "php" => {
            let exe = paths.php_exe(&cfg.php_version);
            if !exe.is_file() {
                return Err(Error::MissingBinary { path: exe });
            }
            Ok((exe, rest))
        }
        "python" => {
            let exe = paths.python_exe(&cfg.python_version);
            if !exe.is_file() {
                return Err(Error::MissingBinary { path: exe });
            }
            Ok((exe, rest))
        }
        "node" => {
            let exe = paths.node_exe();
            if !exe.is_file() {
                return Err(Error::MissingBinary { path: exe });
            }
            Ok((exe, rest))
        }
        script => {
            let cand = root.join(script);
            let canon_root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
            let canon = fs::canonicalize(&cand).map_err(|_| {
                Error::msg(format!("script de cron no encontrado: {script}"))
            })?;
            if !canon.starts_with(&canon_root) {
                return Err(Error::msg("script de cron fuera del sitio"));
            }
            // Ejecutar con el intérprete según extensión, o directamente si es .exe/.cmd/.bat
            let ext = canon
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            match ext.as_str() {
                "php" => {
                    let exe = paths.php_exe(&cfg.php_version);
                    let mut args = vec![canon.to_string_lossy().into_owned()];
                    args.extend(rest);
                    Ok((exe, args))
                }
                "py" => {
                    let exe = paths.python_exe(&cfg.python_version);
                    let mut args = vec![canon.to_string_lossy().into_owned()];
                    args.extend(rest);
                    Ok((exe, args))
                }
                "js" | "mjs" | "cjs" => {
                    let exe = paths.node_exe();
                    let mut args = vec![canon.to_string_lossy().into_owned()];
                    args.extend(rest);
                    Ok((exe, args))
                }
                "exe" | "cmd" | "bat" => Ok((canon, rest)),
                _ => Err(Error::msg(format!(
                    "extensión de cron no permitida '.{ext}' (php|py|js|exe|cmd|bat)"
                ))),
            }
        }
    }
}

fn cron_path(paths: &Paths, cfg: &AppConfig) -> String {
    let mut parts = devkit_path_dirs(paths, cfg);
    if let Ok(existing) = std::env::var("PATH") {
        parts.push(existing);
    }
    parts.join(";")
}

fn devkit_exe() -> Result<PathBuf> {
    std::env::current_exe().map_err(|e| Error::msg(format!("no se pudo resolver devkit.exe: {e}")))
}

fn task_exists(name: &str) -> bool {
    let mut cmd = Command::new("schtasks");
    cmd.args(["/Query", "/TN", name, "/FO", "LIST"]);
    win_spawn::status_silent(&mut cmd)
        .map(|s| s.success())
        .unwrap_or(false)
}

fn create_or_update_task(name: &str, tr: &str, job: &CronJob) -> Result<()> {
    let _ = delete_task(name);
    let mut args = vec![
        "/Create".into(),
        "/TN".into(),
        name.to_string(),
        "/TR".into(),
        tr.to_string(),
        "/RL".into(),
        "LIMITED".into(),
        "/F".into(),
    ];
    match job.every {
        CronEvery::Hourly => {
            args.extend(["/SC".into(), "HOURLY".into(), "/MO".into(), "1".into()]);
        }
        CronEvery::Daily => {
            args.extend(["/SC".into(), "DAILY".into(), "/ST".into(), job.at.clone()]);
        }
        CronEvery::Weekly => {
            args.extend([
                "/SC".into(),
                "WEEKLY".into(),
                "/D".into(),
                "MON".into(),
                "/ST".into(),
                job.at.clone(),
            ]);
        }
    }
    let mut cmd = Command::new("schtasks");
    cmd.args(&args);
    let out = win_spawn::output_hidden(&mut cmd)
        .map_err(|e| Error::msg(format!("schtasks create: {e}")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        return Err(Error::msg(format!(
            "schtasks create falló: {} {}",
            err.trim(),
            stdout.trim()
        )));
    }
    Ok(())
}

fn delete_task(name: &str) -> Result<bool> {
    if !task_exists(name) {
        return Ok(false);
    }
    let mut cmd = Command::new("schtasks");
    cmd.args(["/Delete", "/TN", name, "/F"]);
    let out = win_spawn::output_hidden(&mut cmd)
        .map_err(|e| Error::msg(format!("schtasks delete: {e}")))?;
    Ok(out.status.success())
}

/// Tarea global de comprobación de actualizaciones.
pub fn update_check_task_name() -> &'static str {
    r"DevKit\update-check"
}

pub fn schedule_update_check(weekly_at: &str) -> Result<()> {
    parse_hhmm(weekly_at)?;
    let exe = devkit_exe()?;
    let tr = format!("\"{}\" update check --cache", exe.display());
    let name = update_check_task_name();
    let _ = delete_task(name);
    let mut cmd = Command::new("schtasks");
    cmd.args([
        "/Create",
        "/TN",
        name,
        "/TR",
        &tr,
        "/SC",
        "WEEKLY",
        "/D",
        "MON",
        "/ST",
        weekly_at,
        "/RL",
        "LIMITED",
        "/F",
    ]);
    let out = win_spawn::output_hidden(&mut cmd)
        .map_err(|e| Error::msg(format!("schtasks update-check: {e}")))?;
    if !out.status.success() {
        return Err(Error::msg(format!(
            "no se pudo crear {}: {}",
            name,
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

pub fn disable_update_check() -> Result<bool> {
    delete_task(update_check_task_name())
}

pub fn update_check_scheduled() -> bool {
    task_exists(update_check_task_name())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_argv() {
        let bad = CronJob {
            id: "x".into(),
            every: CronEvery::Daily,
            at: "03:00".into(),
            argv: vec!["php".into(), "artisan; rm".into()],
        };
        assert!(validate_job(&bad).is_err());
        let abs = CronJob {
            id: "x".into(),
            every: CronEvery::Daily,
            at: "03:00".into(),
            argv: vec![r"C:\Windows\System32\cmd.exe".into()],
        };
        assert!(validate_job(&abs).is_err());
    }

    #[test]
    fn accepts_daily_php_job() {
        let ok = CronJob {
            id: "nightly".into(),
            every: CronEvery::Daily,
            at: "04:30".into(),
            argv: vec!["php".into(), "artisan".into(), "schedule:run".into()],
        };
        assert!(validate_job(&ok).is_ok());
    }

    #[test]
    fn parse_time() {
        assert_eq!(parse_hhmm("03:00").unwrap(), (3, 0));
        assert!(parse_hhmm("25:00").is_err());
        assert!(parse_hhmm("3").is_err());
    }

    #[test]
    fn manifest_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let man = CronManifest {
            jobs: vec![CronJob {
                id: "h".into(),
                every: CronEvery::Hourly,
                at: "03:00".into(),
                argv: vec!["node".into(), "scripts/job.js".into()],
            }],
        };
        fs::write(marker_path(dir.path()), toml::to_string_pretty(&man).unwrap()).unwrap();
        let loaded = load_manifest(dir.path()).unwrap();
        assert_eq!(loaded.jobs.len(), 1);
        assert_eq!(loaded.jobs[0].id, "h");
    }

    #[test]
    fn task_name_format() {
        assert_eq!(task_name("blog", "nightly"), r"DevKit\blog\nightly");
    }
}

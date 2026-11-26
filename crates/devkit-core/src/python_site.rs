use crate::error::{Error, Result};
use crate::paths::Paths;
use crate::process_stream::{run_logged, write_log_line};
use crate::site::{parse_site_name, site_root};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const PYTHON_PORT_BASE: u16 = 9100;
pub const PYTHON_PORT_MAX: u16 = 9131;
pub const MARKER_FILE: &str = ".devkit-python";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PythonRuntime {
    Python,
    Flask,
    Fastapi,
    Django,
}

impl PythonRuntime {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::Flask => "flask",
            Self::Fastapi => "fastapi",
            Self::Django => "django",
        }
    }

    pub fn parse(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "python" => Ok(Self::Python),
            "flask" => Ok(Self::Flask),
            "fastapi" => Ok(Self::Fastapi),
            "django" => Ok(Self::Django),
            other => Err(Error::msg(format!(
                "runtime Python desconocido '{other}' (python|flask|fastapi|django)"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonMarker {
    pub version: String,
    pub runtime: PythonRuntime,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PythonPortsFile {
    #[serde(default = "default_base")]
    pub base: u16,
    #[serde(default)]
    pub sites: BTreeMap<String, u16>,
}

fn default_base() -> u16 {
    PYTHON_PORT_BASE
}

impl PythonPortsFile {
    pub fn path(paths: &Paths) -> PathBuf {
        paths.etc.join("devkit").join("python-ports.toml")
    }

    pub fn load(paths: &Paths) -> Result<Self> {
        let p = Self::path(paths);
        if !p.exists() {
            return Ok(Self {
                base: PYTHON_PORT_BASE,
                sites: BTreeMap::new(),
            });
        }
        let text = fs::read_to_string(&p)?;
        let mut file: Self = toml::from_str(&text)?;
        if file.base == 0 {
            file.base = PYTHON_PORT_BASE;
        }
        Ok(file)
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        let p = Self::path(paths);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(p, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}

pub fn marker_path(project: &Path) -> PathBuf {
    project.join(MARKER_FILE)
}

pub fn is_python_site(project: &Path) -> bool {
    marker_path(project).is_file()
}

pub fn read_marker(project: &Path) -> Result<PythonMarker> {
    let text = fs::read_to_string(marker_path(project))?;
    let marker: PythonMarker = toml::from_str(&text)?;
    if !crate::python_version::supported_python_major(&marker.version) {
        return Err(Error::msg(format!(
            "versión Python del sitio no soportada: {}",
            marker.version
        )));
    }
    Ok(marker)
}

pub fn write_marker(project: &Path, marker: &PythonMarker) -> Result<()> {
    fs::write(marker_path(project), toml::to_string_pretty(marker)?)?;
    Ok(())
}

pub fn port_in_range(port: u16) -> bool {
    (PYTHON_PORT_BASE..=PYTHON_PORT_MAX).contains(&port)
}

/// Asigna un puerto estable. Los sitios ya mapeados no se mueven.
pub fn ensure_port(paths: &Paths, site_name: &str) -> Result<u16> {
    let safe = parse_site_name(site_name)?;
    let mut file = PythonPortsFile::load(paths)?;
    if let Some(p) = file.sites.get(&safe).copied() {
        if port_in_range(p) {
            return Ok(p);
        }
        file.sites.remove(&safe);
    }
    let used: std::collections::HashSet<u16> = file.sites.values().copied().collect();
    let mut next = file.base.max(PYTHON_PORT_BASE);
    while used.contains(&next) {
        next = next.saturating_add(1);
        if next > PYTHON_PORT_MAX {
            return Err(Error::msg(format!(
                "no quedan puertos Python libres ({PYTHON_PORT_BASE}..{PYTHON_PORT_MAX})"
            )));
        }
    }
    if !port_in_range(next) {
        return Err(Error::msg("puerto Python fuera de rango"));
    }
    file.sites.insert(safe, next);
    file.save(paths)?;
    Ok(next)
}

pub fn release_port(paths: &Paths, site_name: &str) -> Result<()> {
    let safe = parse_site_name(site_name)?;
    let mut file = PythonPortsFile::load(paths)?;
    if file.sites.remove(&safe).is_some() {
        file.save(paths)?;
    }
    Ok(())
}

pub fn venv_python(project: &Path) -> PathBuf {
    project.join(".venv").join("Scripts").join("python.exe")
}

/// Argv relativo al intérprete del venv (sin el exe).
pub fn build_argv(
    runtime: PythonRuntime,
    project: &Path,
    bind: &str,
    port: u16,
) -> Result<Vec<String>> {
    if !port_in_range(port) {
        return Err(Error::msg(format!(
            "puerto Python {port} fuera de {PYTHON_PORT_BASE}..{PYTHON_PORT_MAX}"
        )));
    }
    if bind != crate::model::LOCALHOST {
        return Err(Error::msg("bind Python debe ser 127.0.0.1"));
    }
    match runtime {
        PythonRuntime::Python => {
            if project.join("app.py").is_file() {
                Ok(vec!["app.py".into()])
            } else {
                Ok(vec![
                    "-m".into(),
                    "http.server".into(),
                    port.to_string(),
                    "--bind".into(),
                    bind.to_string(),
                ])
            }
        }
        PythonRuntime::Flask => Ok(vec!["app.py".into()]),
        PythonRuntime::Fastapi => Ok(vec![
            "-m".into(),
            "uvicorn".into(),
            "app:app".into(),
            "--host".into(),
            bind.to_string(),
            "--port".into(),
            port.to_string(),
        ]),
        PythonRuntime::Django => Ok(vec![
            "manage.py".into(),
            "runserver".into(),
            format!("{bind}:{port}"),
            "--noreload".into(),
        ]),
    }
}

pub fn create_python_site(
    paths: &Paths,
    python_version: &str,
    kind: &str,
    name: &str,
    tld: &str,
    on_line: &mut impl FnMut(&str),
) -> Result<PathBuf> {
    let runtime = PythonRuntime::parse(kind)?;
    if !crate::python_version::supported_python_major(python_version) {
        return Err(Error::msg(format!(
            "Python {python_version} no soportado ({})",
            crate::python_version::SUPPORTED_PYTHON_MAJORS.join(", ")
        )));
    }
    let py = paths.python_exe(python_version);
    if !py.is_file() {
        return Err(Error::MissingBinary { path: py });
    }
    let safe = parse_site_name(name)?;
    let root = site_root(paths, &safe)?;
    if root.exists() {
        return Err(Error::msg(format!("ya existe {}", root.display())));
    }
    fs::create_dir_all(&root)?;

    write_marker(
        &root,
        &PythonMarker {
            version: python_version.to_string(),
            runtime,
        },
    )?;

    match runtime {
        PythonRuntime::Python => {
            write_plain_app(&root, &safe)?;
        }
        PythonRuntime::Flask => {
            write_flask_app(&root, &safe)?;
            fs::write(root.join("requirements.txt"), "flask\n")?;
        }
        PythonRuntime::Fastapi => {
            write_fastapi_app(&root, &safe)?;
            fs::write(root.join("requirements.txt"), "fastapi\nuvicorn\n")?;
        }
        PythonRuntime::Django => {
            // requirements primero; startproject tras venv+pip
            fs::write(root.join("requirements.txt"), "django\n")?;
        }
    }

    write_log_line(on_line, &format!("→ python -m venv .venv ({python_version})"));
    create_venv(&py, &root, on_line)?;

    let req = root.join("requirements.txt");
    if req.is_file() {
        pip_install_requirements(&root, on_line)?;
    }

    if runtime == PythonRuntime::Django {
        write_log_line(on_line, "→ django-admin startproject project .");
        django_startproject(&root, &safe, tld, on_line)?;
    }

    let _ = ensure_port(paths, &safe)?;
    Ok(root)
}

fn create_venv(python: &Path, root: &Path, on_line: &mut impl FnMut(&str)) -> Result<()> {
    let mut cmd = Command::new(python);
    cmd.args(["-m", "venv", ".venv"]).current_dir(root);
    let status = run_logged(&mut cmd, on_line)?;
    if !status.success() {
        return Err(Error::msg("python -m venv falló"));
    }
    if !venv_python(root).is_file() {
        return Err(Error::msg("no apareció .venv/Scripts/python.exe"));
    }
    Ok(())
}

/// Argv fijo de pip (sin el intérprete). Solo `-m pip install -r requirements.txt`.
pub fn pip_install_argv() -> [&'static str; 5] {
    ["-m", "pip", "install", "-r", "requirements.txt"]
}

fn pip_install_requirements(root: &Path, on_line: &mut impl FnMut(&str)) -> Result<()> {
    let py = venv_python(root);
    write_log_line(on_line, "→ python -m pip install -r requirements.txt");
    let mut cmd = Command::new(&py);
    cmd.args(pip_install_argv()).current_dir(root);
    let status = run_logged(&mut cmd, on_line)?;
    if !status.success() {
        return Err(Error::msg("pip install -r requirements.txt falló"));
    }
    Ok(())
}

fn django_startproject(
    root: &Path,
    site_name: &str,
    tld: &str,
    on_line: &mut impl FnMut(&str),
) -> Result<()> {
    let py = venv_python(root);
    let mut cmd = Command::new(&py);
    cmd.args(["-m", "django", "startproject", "project", "."])
        .current_dir(root);
    let status = run_logged(&mut cmd, on_line)?;
    if !status.success() {
        return Err(Error::msg("django startproject falló"));
    }
    patch_django_allowed_hosts(root, site_name, tld)?;
    Ok(())
}

fn patch_django_allowed_hosts(root: &Path, site_name: &str, tld: &str) -> Result<()> {
    let settings = root.join("project").join("settings.py");
    if !settings.is_file() {
        return Ok(());
    }
    let text = fs::read_to_string(&settings)?;
    let host = format!("{site_name}.{tld}");
    let replacement = format!("ALLOWED_HOSTS = ['127.0.0.1', 'localhost', '{host}']");
    let updated = if text.contains("ALLOWED_HOSTS = []") {
        text.replacen("ALLOWED_HOSTS = []", &replacement, 1)
    } else if text.contains("ALLOWED_HOSTS=[]") {
        text.replacen("ALLOWED_HOSTS=[]", &replacement, 1)
    } else {
        text
    };
    fs::write(settings, updated)?;
    Ok(())
}

fn write_plain_app(root: &Path, name: &str) -> Result<()> {
    let body = format!(
        r#"import os
from http.server import BaseHTTPRequestHandler, HTTPServer

BIND = os.environ.get("DEVKIT_BIND", "127.0.0.1")
PORT = int(os.environ.get("DEVKIT_PORT", "9100"))

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        body = b"<h1>{title}</h1><p>DevKit Python</p>"
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, fmt, *args):
        return

if __name__ == "__main__":
    HTTPServer((BIND, PORT), Handler).serve_forever()
"#,
        title = html_escape(name)
    );
    fs::write(root.join("app.py"), body)?;
    Ok(())
}

fn write_flask_app(root: &Path, name: &str) -> Result<()> {
    let body = format!(
        r#"import os
from flask import Flask

app = Flask(__name__)

@app.get("/")
def index():
    return "<h1>{title}</h1><p>Flask en DevKit</p>"

if __name__ == "__main__":
    app.run(
        host=os.environ.get("DEVKIT_BIND", "127.0.0.1"),
        port=int(os.environ.get("DEVKIT_PORT", "9100")),
        debug=False,
        use_reloader=False,
    )
"#,
        title = html_escape(name)
    );
    fs::write(root.join("app.py"), body)?;
    Ok(())
}

fn write_fastapi_app(root: &Path, name: &str) -> Result<()> {
    let body = format!(
        r#"from fastapi import FastAPI
from fastapi.responses import HTMLResponse

app = FastAPI()

@app.get("/", response_class=HTMLResponse)
def index():
    return "<h1>{title}</h1><p>FastAPI en DevKit</p>"
"#,
        title = html_escape(name)
    );
    fs::write(root.join("app.py"), body)?;
    Ok(())
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;

    #[test]
    fn argv_variants() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("app.py"), b"x").unwrap();
        assert_eq!(
            build_argv(PythonRuntime::Python, root, "127.0.0.1", 9100).unwrap(),
            vec!["app.py"]
        );
        fs::remove_file(root.join("app.py")).unwrap();
        let http = build_argv(PythonRuntime::Python, root, "127.0.0.1", 9101).unwrap();
        assert_eq!(http[0], "-m");
        assert_eq!(http[1], "http.server");
        assert!(http.contains(&"--bind".into()));
        assert!(http.contains(&"127.0.0.1".into()));

        let uv = build_argv(PythonRuntime::Fastapi, root, "127.0.0.1", 9102).unwrap();
        assert!(uv.contains(&"uvicorn".into()));
        assert!(uv.contains(&"127.0.0.1".into()));

        let dj = build_argv(PythonRuntime::Django, root, "127.0.0.1", 9103).unwrap();
        assert!(dj.contains(&"runserver".into()));
        assert!(dj.contains(&"127.0.0.1:9103".into()));
        assert!(dj.contains(&"--noreload".into()));

        assert!(build_argv(PythonRuntime::Python, root, "0.0.0.0", 9100).is_err());
        assert!(build_argv(PythonRuntime::Python, root, "127.0.0.1", 80).is_err());
    }

    #[test]
    fn unknown_runtime_rejected() {
        assert!(PythonRuntime::parse("rails").is_err());
    }

    #[test]
    fn ports_stable_when_inserting_earlier_name() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::from_root(dir.path().to_path_buf());
        paths.ensure().unwrap();
        let p_blog = ensure_port(&paths, "blog").unwrap();
        assert_eq!(p_blog, PYTHON_PORT_BASE);
        let p_shop = ensure_port(&paths, "shop").unwrap();
        assert_eq!(p_shop, PYTHON_PORT_BASE + 1);
        // Insertar un nombre que ordena antes no mueve los existentes.
        let p_api = ensure_port(&paths, "api").unwrap();
        assert_eq!(p_api, PYTHON_PORT_BASE + 2);
        assert_eq!(ensure_port(&paths, "blog").unwrap(), p_blog);
        assert_eq!(ensure_port(&paths, "shop").unwrap(), p_shop);
    }

    #[test]
    fn port_out_of_range_helper() {
        assert!(port_in_range(9100));
        assert!(port_in_range(9131));
        assert!(!port_in_range(9099));
        assert!(!port_in_range(9132));
    }

    #[test]
    fn pip_argv_is_fixed_requirements_only() {
        assert_eq!(
            pip_install_argv(),
            ["-m", "pip", "install", "-r", "requirements.txt"]
        );
    }
}

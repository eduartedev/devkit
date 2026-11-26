use crate::error::{Error, Result};
use crate::paths::Paths;
use crate::process_stream::{run_logged, write_log_line};
use crate::site::{parse_site_name, site_root};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const NODE_PORT_BASE: u16 = 9200;
pub const NODE_PORT_MAX: u16 = 9231;
pub const MARKER_FILE: &str = ".devkit-node";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeRuntime {
    Node,
    Express,
}

impl NodeRuntime {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Express => "express",
        }
    }

    pub fn parse(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "node" => Ok(Self::Node),
            "express" => Ok(Self::Express),
            other => Err(Error::msg(format!(
                "runtime Node desconocido '{other}' (node|express)"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMarker {
    pub runtime: NodeRuntime,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodePortsFile {
    #[serde(default = "default_base")]
    pub base: u16,
    #[serde(default)]
    pub sites: BTreeMap<String, u16>,
}

fn default_base() -> u16 {
    NODE_PORT_BASE
}

impl NodePortsFile {
    pub fn path(paths: &Paths) -> PathBuf {
        paths.etc.join("devkit").join("node-ports.toml")
    }

    pub fn load(paths: &Paths) -> Result<Self> {
        let p = Self::path(paths);
        if !p.exists() {
            return Ok(Self {
                base: NODE_PORT_BASE,
                sites: BTreeMap::new(),
            });
        }
        let text = fs::read_to_string(&p)?;
        let mut file: Self = toml::from_str(&text)?;
        if file.base == 0 {
            file.base = NODE_PORT_BASE;
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

pub fn is_node_site(project: &Path) -> bool {
    marker_path(project).is_file()
}

pub fn read_marker(project: &Path) -> Result<NodeMarker> {
    let text = fs::read_to_string(marker_path(project))?;
    Ok(toml::from_str(&text)?)
}

pub fn write_marker(project: &Path, marker: &NodeMarker) -> Result<()> {
    fs::write(marker_path(project), toml::to_string_pretty(marker)?)?;
    Ok(())
}

pub fn port_in_range(port: u16) -> bool {
    (NODE_PORT_BASE..=NODE_PORT_MAX).contains(&port)
}

/// Asigna un puerto estable. Los sitios ya mapeados no se mueven.
pub fn ensure_port(paths: &Paths, site_name: &str) -> Result<u16> {
    let safe = parse_site_name(site_name)?;
    let mut file = NodePortsFile::load(paths)?;
    if let Some(p) = file.sites.get(&safe).copied() {
        if port_in_range(p) {
            return Ok(p);
        }
        file.sites.remove(&safe);
    }
    let used: std::collections::HashSet<u16> = file.sites.values().copied().collect();
    let mut next = file.base.max(NODE_PORT_BASE);
    while used.contains(&next) {
        next = next.saturating_add(1);
        if next > NODE_PORT_MAX {
            return Err(Error::msg(format!(
                "no quedan puertos Node libres ({NODE_PORT_BASE}..{NODE_PORT_MAX})"
            )));
        }
    }
    if !port_in_range(next) {
        return Err(Error::msg("puerto Node fuera de rango"));
    }
    file.sites.insert(safe, next);
    file.save(paths)?;
    Ok(next)
}

pub fn release_port(paths: &Paths, site_name: &str) -> Result<()> {
    let safe = parse_site_name(site_name)?;
    let mut file = NodePortsFile::load(paths)?;
    if file.sites.remove(&safe).is_some() {
        file.save(paths)?;
    }
    Ok(())
}

/// Argv relativo a `node.exe` (sin el exe). Siempre `server.js`.
pub fn build_argv(project: &Path, bind: &str, port: u16) -> Result<Vec<String>> {
    if !port_in_range(port) {
        return Err(Error::msg(format!(
            "puerto Node {port} fuera de {NODE_PORT_BASE}..{NODE_PORT_MAX}"
        )));
    }
    if bind != crate::model::LOCALHOST {
        return Err(Error::msg("bind Node debe ser 127.0.0.1"));
    }
    if !project.join("server.js").is_file() {
        return Err(Error::msg("falta server.js en el sitio Node"));
    }
    Ok(vec!["server.js".into()])
}

pub fn create_node_site(
    paths: &Paths,
    kind: &str,
    name: &str,
    on_line: &mut impl FnMut(&str),
) -> Result<PathBuf> {
    let runtime = NodeRuntime::parse(kind)?;
    let node = paths.node_exe();
    if !node.is_file() {
        return Err(Error::MissingBinary { path: node });
    }
    let safe = parse_site_name(name)?;
    let root = site_root(paths, &safe)?;
    if root.exists() {
        return Err(Error::msg(format!("ya existe {}", root.display())));
    }
    fs::create_dir_all(&root)?;

    write_marker(&root, &NodeMarker { runtime })?;

    match runtime {
        NodeRuntime::Node => {
            write_plain_server(&root, &safe)?;
        }
        NodeRuntime::Express => {
            write_express_files(&root, &safe)?;
            write_log_line(on_line, "→ npm install");
            npm_install(paths, &root, on_line)?;
        }
    }

    let _ = ensure_port(paths, &safe)?;
    Ok(root)
}

/// `npm-cli.js` del Node embebido (evita `npm.cmd` → consola parpadeante).
pub fn npm_cli_js(paths: &Paths) -> Result<PathBuf> {
    let candidates = [
        paths
            .bin
            .join("node")
            .join("node_modules")
            .join("npm")
            .join("bin")
            .join("npm-cli.js"),
        paths
            .bin
            .join("node")
            .join("node_modules")
            .join("npm")
            .join("bin")
            .join("npm.js"),
    ];
    for c in &candidates {
        if c.is_file() {
            return Ok(c.clone());
        }
    }
    Err(Error::MissingBinary {
        path: candidates[0].clone(),
    })
}

pub fn npm_install_argv(cli_js: &Path) -> Vec<String> {
    vec![
        cli_js.to_string_lossy().into_owned(),
        "install".into(),
        "--no-fund".into(),
        "--no-audit".into(),
    ]
}

fn npm_install(paths: &Paths, root: &Path, on_line: &mut impl FnMut(&str)) -> Result<()> {
    let node = paths.node_exe();
    if !node.is_file() {
        return Err(Error::MissingBinary { path: node });
    }
    let cli = npm_cli_js(paths)?;
    let mut cmd = Command::new(&node);
    cmd.args(npm_install_argv(&cli))
        .current_dir(root)
        .env("PATH", prepend_node_path(paths));
    let status = run_logged(&mut cmd, on_line)?;
    if !status.success() {
        return Err(Error::msg("npm install falló"));
    }
    Ok(())
}

fn prepend_node_path(paths: &Paths) -> String {
    let node_dir = paths.bin.join("node");
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let mut parts = vec![node_dir.to_string_lossy().into_owned()];
    parts.push(existing.to_string_lossy().into_owned());
    parts.join(";")
}

fn write_plain_server(root: &Path, name: &str) -> Result<()> {
    let body = format!(
        r#"const http = require("http");

const HOST = process.env.DEVKIT_BIND || "127.0.0.1";
const PORT = Number(process.env.DEVKIT_PORT || "9200");

const server = http.createServer((req, res) => {{
  res.writeHead(200, {{ "Content-Type": "text/html; charset=utf-8" }});
  res.end(
    "<!doctype html><html><head><meta charset=utf-8><title>{name}</title></head>" +
      "<body><h1>{name}</h1><p>Node · DevKit</p></body></html>"
  );
}});

server.listen(PORT, HOST, () => {{
  console.log(`DevKit Node listening on http://${{HOST}}:${{PORT}}`);
}});
"#
    );
    fs::write(root.join("server.js"), body)?;
    Ok(())
}

fn write_express_files(root: &Path, name: &str) -> Result<()> {
    let pkg = format!(
        r#"{{
  "name": "{name}",
  "private": true,
  "version": "0.0.0",
  "main": "server.js",
  "dependencies": {{
    "express": "^4.21.0"
  }}
}}
"#
    );
    fs::write(root.join("package.json"), pkg)?;
    let body = format!(
        r#"const express = require("express");

const HOST = process.env.DEVKIT_BIND || "127.0.0.1";
const PORT = Number(process.env.DEVKIT_PORT || "9200");

const app = express();
app.get("/", (_req, res) => {{
  res.type("html").send(
    "<!doctype html><html><head><meta charset=utf-8><title>{name}</title></head>" +
      "<body><h1>{name}</h1><p>Express · DevKit</p></body></html>"
  );
}});

app.listen(PORT, HOST, () => {{
  console.log(`DevKit Express listening on http://${{HOST}}:${{PORT}}`);
}});
"#
    );
    fs::write(root.join("server.js"), body)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn runtime_parse() {
        assert_eq!(NodeRuntime::parse("node").unwrap(), NodeRuntime::Node);
        assert_eq!(NodeRuntime::parse("express").unwrap(), NodeRuntime::Express);
        assert!(NodeRuntime::parse("next").is_err());
    }

    #[test]
    fn build_argv_requires_server_js_and_localhost() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        assert!(build_argv(root, "127.0.0.1", 9200).is_err());
        fs::write(root.join("server.js"), "ok").unwrap();
        assert_eq!(
            build_argv(root, "127.0.0.1", 9200).unwrap(),
            vec!["server.js".to_string()]
        );
        assert!(build_argv(root, "0.0.0.0", 9200).is_err());
        assert!(build_argv(root, "127.0.0.1", 9100).is_err());
    }

    #[test]
    fn port_assign_stable_and_release() {
        let dir = tempdir().unwrap();
        let paths = Paths::from_root(dir.path().to_path_buf());
        paths.ensure().unwrap();
        let a = ensure_port(&paths, "api").unwrap();
        let b = ensure_port(&paths, "api").unwrap();
        assert_eq!(a, b);
        assert!(port_in_range(a));
        let c = ensure_port(&paths, "web").unwrap();
        assert_ne!(a, c);
        release_port(&paths, "api").unwrap();
        let again = ensure_port(&paths, "other").unwrap();
        assert_eq!(again, a);
    }

    #[test]
    fn marker_roundtrip() {
        let dir = tempdir().unwrap();
        write_marker(
            dir.path(),
            &NodeMarker {
                runtime: NodeRuntime::Express,
            },
        )
        .unwrap();
        assert!(is_node_site(dir.path()));
        let m = read_marker(dir.path()).unwrap();
        assert_eq!(m.runtime, NodeRuntime::Express);
    }

    #[test]
    fn write_plain_creates_server() {
        let dir = tempdir().unwrap();
        write_plain_server(dir.path(), "demo").unwrap();
        let text = fs::read_to_string(dir.path().join("server.js")).unwrap();
        assert!(text.contains("DEVKIT_PORT"));
        assert!(text.contains("demo"));
    }

    #[test]
    fn npm_cli_js_and_install_argv_skip_cmd() {
        let dir = tempdir().unwrap();
        let paths = Paths::from_root(dir.path().to_path_buf());
        paths.ensure().unwrap();
        let cli = paths
            .bin
            .join("node")
            .join("node_modules")
            .join("npm")
            .join("bin")
            .join("npm-cli.js");
        fs::create_dir_all(cli.parent().unwrap()).unwrap();
        fs::write(&cli, "module.exports = {}").unwrap();
        fs::write(paths.bin.join("node").join("npm.cmd"), "@echo off").unwrap();
        assert_eq!(npm_cli_js(&paths).unwrap(), cli);
        let argv = npm_install_argv(&cli);
        assert_eq!(argv[0], cli.to_string_lossy());
        assert_eq!(&argv[1..], ["install", "--no-fund", "--no-audit"]);
        assert!(!argv.iter().any(|a| a.ends_with("npm.cmd")));
    }
}

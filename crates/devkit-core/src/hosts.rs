use crate::error::{Error, Result};
use crate::model::Site;
use crate::model::LOCALHOST;
use std::fs;
use std::path::PathBuf;

const BEGIN: &str = "# BEGIN DEVKIT";
const END: &str = "# END DEVKIT";

pub fn hosts_path() -> PathBuf {
    PathBuf::from(r"C:\Windows\System32\drivers\etc\hosts")
}

pub fn render_block(sites: &[Site]) -> String {
    let mut lines = vec![BEGIN.to_string()];
    lines.push(format!("{LOCALHOST} localhost"));
    for site in sites {
        lines.push(format!("{LOCALHOST} {}", site.hostname));
    }
    lines.push(END.to_string());
    lines.join("\r\n") + "\r\n"
}

pub fn merge_hosts(original: &str, sites: &[Site]) -> String {
    let block = render_block(sites);
    if let (Some(a), Some(b)) = (original.find(BEGIN), original.find(END)) {
        if b >= a {
            let end = b + END.len();
            let mut out = String::new();
            out.push_str(&original[..a]);
            out.push_str(&block);
            if end < original.len() {
                out.push_str(original[end..].trim_start_matches(['\r', '\n']));
            }
            return out;
        }
    }
    let mut out = original.trim_end().to_string();
    if !out.is_empty() {
        out.push_str("\r\n\r\n");
    }
    out.push_str(&block);
    out
}

pub fn apply_hosts(sites: &[Site]) -> Result<()> {
    let path = hosts_path();
    let original = fs::read_to_string(&path).unwrap_or_default();
    let merged = merge_hosts(&original, sites);
    fs::write(&path, merged).map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            Error::msg(
                "no hay permiso para editar el archivo hosts (hace falta UAC). El DNS local es la vía preferida.",
            )
        } else {
            Error::from(e)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Site;
    use std::path::PathBuf;

    fn site(name: &str) -> Site {
        Site {
            name: name.into(),
            hostname: format!("{name}.test"),
            root: PathBuf::from(format!(r"C:\DevKit\www\{name}")),
            php_version: "8.3".into(),
            ssl: false,
            url: String::new(),
        }
    }

    #[test]
    fn replaces_existing_block() {
        let orig = "10.0.1.3 other\r\n# BEGIN DEVKIT\r\n127.0.0.1 old.test\r\n# END DEVKIT\r\n";
        let out = merge_hosts(orig, &[site("blog")]);
        assert!(out.contains("127.0.0.1 blog.test"));
        assert!(!out.contains("old.test"));
        assert!(out.contains("10.0.1.3 other"));
        assert_eq!(out.matches(BEGIN).count(), 1);
    }
}

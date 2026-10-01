use crate::error::{Error, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct Artifact {
    pub id: String,
    pub url: String,
    pub sha256: Option<String>,
    pub dest_dir: PathBuf,
    pub kind: ArtifactKind,
}

pub enum ArtifactKind {
    Zip,
    Exe { filename: String },
    File { filename: String },
}

pub fn internet_available() -> bool {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(8))
        .user_agent("DevKit/0.1 (+local-dev)")
        .build();
    agent.get("https://nginx.org/").call().is_ok()
}

pub fn download_verified(url: &str, dest: &Path, expected_sha256: Option<&str>) -> Result<String> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(180))
        .user_agent("DevKit/0.1 (+local-dev)")
        .build();
    let response = agent
        .get(url)
        .call()
        .map_err(|e| Error::Http(format!("{url}: {e}")))?;
    let mut reader = response.into_reader();
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    let actual = hex::encode(Sha256::digest(&bytes));
    if let Some(exp) = expected_sha256 {
        if !exp.eq_ignore_ascii_case(&actual) {
            return Err(Error::Checksum {
                artifact: url.to_string(),
                expected: exp.to_ascii_lowercase(),
                actual,
            });
        }
    }
    let mut f = fs::File::create(dest)?;
    f.write_all(&bytes)?;
    Ok(actual)
}

pub fn unzip_safe(zip_path: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    let file = fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for i in 0..archive.len() {
        let mut item = archive.by_index(i)?;
        let Some(enclosed) = item.enclosed_name() else {
            return Err(Error::ZipSlip {
                name: item.name().to_string(),
            });
        };
        let out = dest.join(enclosed);
        let dest_canon = dest.canonicalize().unwrap_or_else(|_| dest.to_path_buf());
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        if item.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        let mut outfile = fs::File::create(&out)?;
        std::io::copy(&mut item, &mut outfile)?;
        let _ = dest_canon;
    }
    unwrap_single_dir(dest)?;
    Ok(())
}

fn unwrap_single_dir(dest: &Path) -> Result<()> {
    let entries: Vec<_> = fs::read_dir(dest)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name() != ".gitkeep")
        .collect();
    if entries.len() != 1 {
        return Ok(());
    }
    let only = entries[0].path();
    if !only.is_dir() {
        return Ok(());
    }
    let tmp = dest
        .parent()
        .unwrap_or(dest)
        .join(format!(".unwrap-{}", dest.file_name().unwrap().to_string_lossy()));
    if tmp.exists() {
        fs::remove_dir_all(&tmp)?;
    }
    fs::rename(&only, &tmp)?;
    for child in fs::read_dir(&tmp)? {
        let child = child?;
        let target = dest.join(child.file_name());
        fs::rename(child.path(), target)?;
    }
    let _ = fs::remove_dir_all(tmp);
    Ok(())
}

pub fn github_latest_asset(repo: &str, name_contains: &[&str]) -> Result<(String, Option<String>)> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .user_agent("DevKit/0.1")
        .build();
    let body = agent
        .get(&url)
        .call()
        .map_err(|e| Error::Http(format!("{url}: {e}")))?
        .into_string()
        .map_err(|e| Error::Http(e.to_string()))?;
    let json: serde_json::Value = serde_json::from_str(&body)?;
    let assets = json
        .get("assets")
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_default();
    for asset in assets {
        let name = asset.get("name").and_then(|n| n.as_str()).unwrap_or("");
        let lower = name.to_ascii_lowercase();
        if name_contains.iter().all(|p| lower.contains(&p.to_ascii_lowercase())) {
            let url = asset
                .get("browser_download_url")
                .and_then(|u| u.as_str())
                .ok_or_else(|| Error::msg(format!("asset sin url en {repo}")))?;
            let sha = asset
                .get("digest")
                .and_then(|d| d.as_str())
                .and_then(|d| d.strip_prefix("sha256:"))
                .map(|s| s.to_string());
            return Ok((url.to_string(), sha));
        }
    }
    Err(Error::msg(format!(
        "no hay asset que coincida en {repo}: {name_contains:?}"
    )))
}

pub fn php_nts_from_releases(major: &str) -> Result<(String, String, String)> {
    let url = "https://windows.php.net/downloads/releases/releases.json";
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .user_agent("DevKit/0.1")
        .build();
    let body = agent
        .get(url)
        .call()
        .map_err(|e| Error::Http(format!("{url}: {e}")))?
        .into_string()
        .map_err(|e| Error::Http(e.to_string()))?;
    let json: serde_json::Value = serde_json::from_str(&body)?;
    let rel = json
        .get(major)
        .ok_or_else(|| Error::msg(format!("PHP {major} no está en releases.json")))?;
    let version = rel
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or(major)
        .to_string();
    let keys = ["nts-vs17-x64", "nts-vs16-x64", "nts-vc15-x64"];
    for key in keys {
        if let Some(zip) = rel.get(key).and_then(|k| k.get("zip")) {
            let path = zip
                .get("path")
                .and_then(|p| p.as_str())
                .ok_or_else(|| Error::msg("php zip path"))?;
            let sha = zip
                .get("sha256")
                .and_then(|p| p.as_str())
                .ok_or_else(|| Error::msg("php zip sha256"))?;
            let full = format!("https://windows.php.net/downloads/releases/{path}");
            return Ok((version, full, sha.to_string()));
        }
    }
    Err(Error::msg(format!("no hay build NTS x64 para PHP {major}")))
}

pub fn node_lts_zip() -> Result<(String, String, String)> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .user_agent("DevKit/0.1")
        .build();
    let body = agent
        .get("https://nodejs.org/dist/index.json")
        .call()
        .map_err(|e| Error::Http(e.to_string()))?
        .into_string()
        .map_err(|e| Error::Http(e.to_string()))?;
    let index: serde_json::Value = serde_json::from_str(&body)?;
    let arr = index.as_array().ok_or_else(|| Error::msg("index.json node"))?;
    let lts = arr
        .iter()
        .find(|v| v.get("lts").map(|l| !l.is_null() && l.as_bool() != Some(false)).unwrap_or(false) && v.get("lts").is_some())
        .or_else(|| arr.iter().find(|v| v.get("lts").map(|l| l.is_string()).unwrap_or(false)))
        .ok_or_else(|| Error::msg("no hay LTS en nodejs.org"))?;
    let version = lts
        .get("version")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::msg("node version"))?
        .to_string();
    let shasums_url = format!("https://nodejs.org/dist/{version}/SHASUMS256.txt");
    let shasums = agent
        .get(&shasums_url)
        .call()
        .map_err(|e| Error::Http(e.to_string()))?
        .into_string()
        .map_err(|e| Error::Http(e.to_string()))?;
    let filename = format!("node-{version}-win-x64.zip");
    for line in shasums.lines() {
        let mut parts = line.split_whitespace();
        if let (Some(hash), Some(file)) = (parts.next(), parts.next()) {
            if file == filename || file.ends_with(&filename) {
                let url = format!("https://nodejs.org/dist/{version}/{filename}");
                return Ok((version, url, hash.to_string()));
            }
        }
    }
    Err(Error::msg(format!("no está {filename} en SHASUMS256")))
}

pub fn composer_latest() -> Result<(String, String)> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .user_agent("DevKit/0.1")
        .build();
    let sha = agent
        .get("https://getcomposer.org/download/latest-stable/composer.phar.sha256sum")
        .call()
        .map_err(|e| Error::Http(e.to_string()))?
        .into_string()
        .map_err(|e| Error::Http(e.to_string()))?;
    let hash = sha
        .split_whitespace()
        .next()
        .ok_or_else(|| Error::msg("composer sha256sum vacío"))?
        .to_string();
    Ok((
        "https://getcomposer.org/download/latest-stable/composer.phar".into(),
        hash,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn write_zip(path: &Path, files: &[(&str, &[u8])]) {
        let file = fs::File::create(path).unwrap();
        let mut archive = ZipWriter::new(file);
        let opts = SimpleFileOptions::default();
        for (name, bytes) in files {
            archive.start_file(*name, opts).unwrap();
            archive.write_all(bytes).unwrap();
        }
        archive.finish().unwrap();
    }

    #[test]
    fn unzip_safe_rejects_parent_path() {
        let root = tempfile::tempdir().unwrap();
        let zip_path = root.path().join("slip.zip");
        write_zip(&zip_path, &[("../evil.txt", b"nope")]);
        let dest = root.path().join("out");
        let err = unzip_safe(&zip_path, &dest).unwrap_err();
        assert!(matches!(err, Error::ZipSlip { .. }), "{err}");
        assert!(!root.path().join("evil.txt").exists());
        assert!(!dest.join("evil.txt").exists());
    }

    #[test]
    fn unzip_safe_flattens_single_root_dir() {
        let root = tempfile::tempdir().unwrap();
        let zip_path = root.path().join("nginx.zip");
        write_zip(
            &zip_path,
            &[
                ("nginx-1.28.2/nginx.exe", b"nginx"),
                ("nginx-1.28.2/conf/nginx.conf", b"conf"),
            ],
        );
        let dest = root.path().join("nginx");
        unzip_safe(&zip_path, &dest).unwrap();
        assert_eq!(fs::read(dest.join("nginx.exe")).unwrap(), b"nginx");
        assert_eq!(fs::read(dest.join("conf").join("nginx.conf")).unwrap(), b"conf");
        assert!(!dest.join("nginx-1.28.2").exists());
    }
}

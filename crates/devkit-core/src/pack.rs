use crate::error::Result;
use crate::paths::Paths;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

pub fn pack(paths: &Paths, out: Option<PathBuf>) -> Result<PathBuf> {
    let dest = out.unwrap_or_else(|| paths.tmp.join("DevKit-portable.zip"));
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = fs::File::create(&dest)?;
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    add_dir(&mut zip, &paths.root, &paths.root, opts)?;
    zip.finish()?;
    Ok(dest)
}

fn skip(rel: &str) -> bool {
    let n = rel.replace('\\', "/");
    n.contains("/target/")
        || n.starts_with("target/")
        || n.contains("/node_modules/")
        || n.starts_with("tmp/")
        || n.starts_with(".git/")
        || n.ends_with(".zip")
}

fn add_dir(
    zip: &mut ZipWriter<fs::File>,
    root: &Path,
    dir: &Path,
    opts: SimpleFileOptions,
) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        if skip(&rel) {
            continue;
        }
        if path.is_dir() {
            add_dir(zip, root, &path, opts)?;
        } else {
            zip.start_file(rel.replace('\\', "/"), opts)?;
            let mut f = fs::File::open(&path)?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)?;
            zip.write_all(&buf)?;
        }
    }
    Ok(())
}

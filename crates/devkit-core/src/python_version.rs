pub const SUPPORTED_PYTHON_MAJORS: &[&str] = &["3.12", "3.13", "3.14"];
pub const DEFAULT_PYTHON_MAJOR: &str = "3.13";

pub fn supported_python_major(major: &str) -> bool {
    SUPPORTED_PYTHON_MAJORS.iter().any(|m| *m == major)
}

/// Triple de python-build-standalone para la arquitectura del proceso.
pub fn windows_python_triple(arch: &str) -> Result<&'static str, String> {
    match arch {
        "x86_64" => Ok("x86_64-pc-windows-msvc"),
        "aarch64" => Ok("aarch64-pc-windows-msvc"),
        other => Err(format!(
            "arquitectura no soportada para Python portable: {other} (solo x86_64 y aarch64)"
        )),
    }
}

pub fn host_python_triple() -> Result<&'static str, String> {
    windows_python_triple(std::env::consts::ARCH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn majors_and_triples() {
        assert!(supported_python_major("3.13"));
        assert!(!supported_python_major("3.11"));
        assert_eq!(
            windows_python_triple("x86_64").unwrap(),
            "x86_64-pc-windows-msvc"
        );
        assert_eq!(
            windows_python_triple("aarch64").unwrap(),
            "aarch64-pc-windows-msvc"
        );
        assert!(windows_python_triple("x86").is_err());
    }
}

use std::fmt;
use std::io;
use std::path::PathBuf;
use std::process::ExitStatus;

#[derive(Debug)]
pub enum Error {
    Message(String),
    Io(io::Error),
    Json(serde_json::Error),
    TomlDe(toml::de::Error),
    TomlSer(toml::ser::Error),
    Http(String),
    Zip(zip::result::ZipError),
    Process { name: String, status: Option<ExitStatus> },
    PortOccupied { service: String, port: u16, hint: String },
    BindForbidden { got: String },
    InvalidSiteName { raw: String, reason: String },
    MissingBinary { path: PathBuf },
    Checksum { artifact: String, expected: String, actual: String },
    ZipSlip { name: String },
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn msg(m: impl Into<String>) -> Self {
        Self::Message(m.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Message(m) => write!(f, "{m}"),
            Error::Io(e) => write!(f, "E/S: {e}"),
            Error::Json(e) => write!(f, "JSON: {e}"),
            Error::TomlDe(e) => write!(f, "TOML: {e}"),
            Error::TomlSer(e) => write!(f, "TOML: {e}"),
            Error::Http(e) => write!(f, "HTTP: {e}"),
            Error::Zip(e) => write!(f, "ZIP: {e}"),
            Error::Process { name, status } => {
                write!(f, "proceso {name} terminó ({status:?})")
            }
            Error::PortOccupied { service, port, hint } => {
                write!(f, "{service} no arrancó: puerto {port} ocupado — {hint}")
            }
            Error::BindForbidden { got } => {
                write!(f, "bind prohibido (solo 127.0.0.1): {got}")
            }
            Error::InvalidSiteName { raw, reason } => {
                write!(f, "nombre de sitio inválido '{raw}': {reason}")
            }
            Error::MissingBinary { path } => {
                write!(f, "falta el binario {}; ejecuta `devkit setup`", path.display())
            }
            Error::Checksum {
                artifact,
                expected,
                actual,
            } => write!(
                f,
                "checksum SHA-256 de {artifact} no coincide (esperado {expected}, obtenido {actual})"
            ),
            Error::ZipSlip { name } => write!(f, "zip rechazado (ruta insegura): {name}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<toml::de::Error> for Error {
    fn from(value: toml::de::Error) -> Self {
        Self::TomlDe(value)
    }
}

impl From<toml::ser::Error> for Error {
    fn from(value: toml::ser::Error) -> Self {
        Self::TomlSer(value)
    }
}

impl From<zip::result::ZipError> for Error {
    fn from(value: zip::result::ZipError) -> Self {
        Self::Zip(value)
    }
}

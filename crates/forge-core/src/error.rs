use std::io;
use std::path::Path;
use std::process::ExitStatus;

#[derive(Debug, thiserror::Error)]
pub enum ForgeError {
    #[error("{0}")]
    Host(String),
    #[error("{0}")]
    Image(String),
    #[error("{0}")]
    Verify(String),
    #[error("{0}")]
    Virt(String),
    #[error("{0}")]
    Role(String),
    #[error("{0}")]
    Ownership(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    AlreadyExists(String),
    #[error("{0}")]
    InvalidInput(String),
    #[error("{0}")]
    NotThisCut(String),
    #[error("{0}")]
    Io(#[from] io::Error),
}

impl ForgeError {
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::InvalidInput(_) | Self::AlreadyExists(_) => 2,
            _ => 1,
        }
    }
}

pub type Result<T> = std::result::Result<T, ForgeError>;

pub(crate) fn io_path(err: &io::Error, path: &Path, what: &str) -> ForgeError {
    let extra = if err.kind() == io::ErrorKind::PermissionDenied {
        " — run forge as yourself (not `sudo forge`); sudo is only for /var/lib/forge"
    } else {
        ""
    };
    ForgeError::Host(format!("{what} {}: {err}{extra}", path.display()))
}

pub(crate) fn command_fail(name: &str, status: ExitStatus, stderr: &[u8]) -> ForgeError {
    let stderr = String::from_utf8_lossy(stderr);
    let stderr = stderr.trim();
    if stderr.is_empty() {
        ForgeError::Host(format!("{name} failed with {status}"))
    } else {
        ForgeError::Host(format!("{name} failed: {stderr}"))
    }
}

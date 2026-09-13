use std::fs;
use std::path::Path;

use crate::cmd;
use crate::error::{ForgeError, Result};
use crate::paths::ForgePaths;
use crate::progress::{self, Progress};

pub fn import_and_pin(
    paths: &ForgePaths,
    name: &str,
    armored: &str,
    fingerprint: &str,
    progress: &Progress,
) -> Result<std::path::PathBuf> {
    paths.ensure_user()?;
    progress::message(
        progress,
        format!("Pinning OpenPGP key {name} fingerprint {fingerprint}"),
    );
    let key_asc = paths.keys_root().join(format!("{name}.asc"));
    fs::write(&key_asc, armored)?;
    let homedir = paths.keys_root().join(name);
    fs::create_dir_all(&homedir)?;
    crate::paths::chmod(&homedir, 0o700)?;
    cmd::run(
        "gpg",
        &[
            "--batch",
            "--yes",
            "--homedir",
            path(&homedir)?,
            "--import",
            path(&key_asc)?,
        ],
    )?;
    let listing = cmd::run_checked(
        "gpg",
        &[
            "--batch",
            "--homedir",
            path(&homedir)?,
            "--with-colons",
            "--fingerprint",
        ],
    )?;
    let expected = normalize_fpr(fingerprint);
    let found = listing.lines().any(|line| {
        line.starts_with("fpr:") && normalize_fpr(&line.to_ascii_uppercase()).contains(&expected)
    });
    if !found {
        return Err(ForgeError::Verify(format!(
            "{name} signing key fingerprint mismatch (want {fingerprint})"
        )));
    }
    Ok(homedir)
}

pub fn verify_clearsign(homedir: &Path, signed: &Path, progress: &Progress) -> Result<String> {
    progress::message(
        progress,
        format!("Verifying clearsigned {}", signed.display()),
    );
    let output = cmd::command("gpg")
        .args([
            "--batch",
            "--yes",
            "--homedir",
            path(homedir)?,
            "--decrypt",
            path(signed)?,
        ])
        .output()
        .map_err(|error| ForgeError::Verify(format!("gpg: {error}")))?;
    if !output.status.success() {
        return Err(ForgeError::Verify(format!(
            "OpenPGP verify failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn normalize_fpr(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_hexdigit)
        .collect::<String>()
        .to_ascii_uppercase()
}

fn path(p: &Path) -> Result<&str> {
    p.to_str()
        .ok_or_else(|| ForgeError::Image("non-utf8 path".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{TSURUGI_KEY_ASC, TSURUGI_KEY_FPR};

    #[test]
    fn vendored_tsurugi_key_matches_pin() {
        let dir = std::env::temp_dir().join(format!("forge-key-{}", uuid::Uuid::new_v4()));
        let paths = ForgePaths::under(dir.clone(), false);
        let homedir = import_and_pin(
            &paths,
            "tsurugi",
            TSURUGI_KEY_ASC,
            TSURUGI_KEY_FPR,
            &crate::progress::noop,
        )
        .expect("import");
        assert!(homedir.is_dir());
        let _ = fs::remove_dir_all(dir);
    }
}

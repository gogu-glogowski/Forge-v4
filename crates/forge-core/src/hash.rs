use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256, Sha512};

use crate::error::{Result, io_path};
use crate::progress::{ByteProgress, Progress};

#[derive(Debug, Clone, Copy)]
pub enum HashKind {
    Sha256,
    Sha512,
}

pub fn sha256_file(path: &Path, progress: &Progress) -> Result<String> {
    hash_file(path, progress, HashKind::Sha256)
}

pub fn sha512_file(path: &Path, progress: &Progress) -> Result<String> {
    hash_file(path, progress, HashKind::Sha512)
}

pub fn hash_file(path: &Path, progress: &Progress, kind: HashKind) -> Result<String> {
    let total = fs::metadata(path).ok().map(|meta| meta.len());
    let mut file = File::open(path).map_err(|err| io_path(&err, path, "cannot read"))?;
    let mut buf = vec![0_u8; 1024 * 1024];
    let mut done = 0_u64;
    let meter = ByteProgress::new(progress, "hash");
    let mut sha256 = Sha256::new();
    let mut sha512 = Sha512::new();
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        match kind {
            HashKind::Sha256 => sha256.update(&buf[..n]),
            HashKind::Sha512 => sha512.update(&buf[..n]),
        }
        done += n as u64;
        meter.emit(done, total);
    }
    Ok(match kind {
        HashKind::Sha256 => hex::encode(sha256.finalize()),
        HashKind::Sha512 => hex::encode(sha512.finalize()),
    })
}

#[must_use]
pub fn checksum_for(sums: &str, pred: impl Fn(&str) -> bool) -> Option<(String, String)> {
    for line in sums.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        if pred(name) {
            return Some((name.to_owned(), hash.to_ascii_lowercase()));
        }
    }
    None
}

/// SHA-256 as published on the SANS SIFT page (`sha256 = <hex>`).
#[must_use]
pub fn sift_sha256_from_page(html: &str) -> Option<String> {
    for line in html.lines() {
        let lower = line.to_ascii_lowercase();
        let Some(rest) = lower
            .split("sha256")
            .nth(1)
            .and_then(|s| s.split('=').nth(1))
        else {
            continue;
        };
        let hex: String = rest
            .chars()
            .filter(char::is_ascii_hexdigit)
            .take(64)
            .collect();
        if hex.len() == 64 {
            return Some(hex);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tsurugi_ova_line() {
        let sums = "\
658687dfbe65e8ca408c73669a6f3d04148074fe7441e034c6ec18c67ca28448\
328bc84aca8271ca3718fad2afb4269807cf014253cf791f49f3ad5ed4f9e4f3 tsurugi_linux_26.03.ova\n";
        let (name, hash) =
            checksum_for(sums, |n| n.ends_with(".ova") && n.contains("tsurugi")).expect("ova line");
        assert_eq!(name, "tsurugi_linux_26.03.ova");
        assert!(hash.starts_with("658687df"));
        assert_eq!(hash.len(), 128);
    }

    #[test]
    fn unreadable_file_names_the_path() {
        let dir = std::env::temp_dir().join(format!("forge-hash-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secret.qcow2");
        fs::write(&path, b"nope").unwrap();
        let mut perms = fs::metadata(&path).unwrap().permissions();
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o000);
        fs::set_permissions(&path, perms).unwrap();
        let err = sha256_file(&path, &crate::progress::noop).unwrap_err();
        let text = err.to_string();
        assert!(text.contains("secret.qcow2"), "{text}");
        assert!(
            text.contains("sudo forge") || text.contains("Permission denied"),
            "{text}"
        );
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&path, perms).unwrap();
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn parse_sift_sha256_from_sans_html() {
        let html = "\
Hash Values
md5 = 81029da183c0dc0dd7cd2b5bb04bfda0
sha1 = b87b8fa5c46ab55bbb4bf414de9519688901b5a9
sha256 = 69960210f92f2329ea69c648c971c23d6bd42568de66586ee4b8273797b9c860
";
        assert_eq!(
            sift_sha256_from_page(html).as_deref(),
            Some("69960210f92f2329ea69c648c971c23d6bd42568de66586ee4b8273797b9c860")
        );
    }
}

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256, Sha512};

use crate::error::Result;
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
    let mut file = File::open(path)?;
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
}

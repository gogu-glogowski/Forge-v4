//! HTTP(S) fetch: curl with resume, on-disk cache, optional verify-and-refetch.
//!
//! This module is Forge-shaped (`cmd` / `progress` / `ForgeError`) but otherwise
//! standalone — copy it with those three adapters to reuse in another tool.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cmd;
use crate::error::{ForgeError, Result};
use crate::progress::{self, ByteProgress, Progress};

pub const USER_AGENT: &str = "forge/4.0";

/// What to do when `dest` already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// Always hit the network (checksums, signatures, HTML). Resume `.part`.
    Refresh,
    /// Skip the network if `dest` is a non-empty file (large artifacts).
    Cache,
}

/// Fetch `url` into `dest`. `file://` and local paths copy.
pub fn fetch(url: &str, dest: &Path, policy: Policy, progress: &Progress) -> Result<PathBuf> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    if policy == Policy::Cache && is_complete(dest) {
        progress::message(
            progress,
            format!("using cached {} (skip download)", dest.display()),
        );
        return Ok(dest.to_path_buf());
    }
    if let Some(local) = url.strip_prefix("file://") {
        fs::copy(local, dest)?;
        return Ok(dest.to_path_buf());
    }
    if Path::new(url).exists() {
        fs::copy(url, dest)?;
        return Ok(dest.to_path_buf());
    }
    let tmp = part_file(dest);
    if cmd::exists("curl") {
        download_curl(url, &tmp, progress)?;
    } else {
        download_ureq(url, &tmp, progress)?;
    }
    fs::rename(&tmp, dest)?;
    Ok(dest.to_path_buf())
}

/// Use cache if `verify` succeeds; otherwise fetch once and verify again.
///
/// On a stale cached file: delete `dest` (leave `.part` for resume) and re-get.
pub fn fetch_verified(
    url: &str,
    dest: &Path,
    progress: &Progress,
    verify: impl Fn(&Path) -> Result<()>,
) -> Result<PathBuf> {
    if is_complete(dest) {
        progress::message(progress, format!("checking cached {}", dest.display()));
        match verify(dest) {
            Ok(()) => {
                progress::message(progress, format!("cached {} ok", dest.display()));
                return Ok(dest.to_path_buf());
            }
            Err(error) => {
                if same_local_source(url, dest) {
                    return Err(error);
                }
                progress::message(
                    progress,
                    format!(
                        "cached {} failed verification ({error}); re-fetching",
                        dest.display()
                    ),
                );
                fs::remove_file(dest)?;
            }
        }
    }
    fetch(url, dest, Policy::Refresh, progress)?;
    verify(dest)?;
    Ok(dest.to_path_buf())
}

#[must_use]
pub fn is_complete(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.len() > 0)
}

#[must_use]
pub fn part_file(dest: &Path) -> PathBuf {
    dest.with_extension("part")
}

#[must_use]
pub fn curl_args<'a>(url: &'a str, tmp: &'a str) -> Vec<&'a str> {
    vec![
        "-fL",
        "--retry",
        "8",
        "--retry-delay",
        "3",
        "--retry-all-errors",
        "-C",
        "-",
        "-A",
        USER_AGENT,
        "--connect-timeout",
        "30",
        "-o",
        tmp,
        url,
    ]
}

fn download_curl(url: &str, tmp: &Path, progress: &Progress) -> Result<()> {
    let tmp_s = utf8(tmp)?;
    let already = fs::metadata(tmp).map(|m| m.len()).unwrap_or(0);
    if already > 0 {
        progress::message(
            progress,
            format!(
                "GET {url} (curl resume from {})",
                crate::progress::format_bytes(already)
            ),
        );
    } else {
        progress::message(progress, format!("GET {url} (curl)"));
    }
    let args = curl_args(url, tmp_s);
    let status = cmd::command("curl")
        .args(&args)
        .status()
        .map_err(|error| ForgeError::Image(format!("cannot run curl: {error}")))?;
    if !status.success() {
        return Err(ForgeError::Image(format!(
            "curl failed for {url} ({status}). Re-run — the .part file resumes."
        )));
    }
    Ok(())
}

fn download_ureq(url: &str, tmp: &Path, progress: &Progress) -> Result<()> {
    progress::message(
        progress,
        format!("GET {url} (ureq fallback — install curl for resume)"),
    );
    let already = fs::metadata(tmp).map(|m| m.len()).unwrap_or(0);
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(30))
        .timeout_read(Duration::from_secs(6 * 60 * 60))
        .timeout_write(Duration::from_secs(60))
        .user_agent(USER_AGENT)
        .build();
    let mut request = agent.get(url);
    if already > 0 {
        request = request.set("Range", &format!("bytes={already}-"));
    }
    let response = request
        .call()
        .map_err(|error| ForgeError::Image(format!("download {url}: {error}")))?;
    let status = response.status();
    if already > 0 && status != 206 && status != 200 {
        return Err(ForgeError::Image(format!(
            "resume {url}: HTTP {status} (want 206). Delete {} and retry",
            tmp.display()
        )));
    }
    let append = already > 0 && status == 206;
    let total = if append {
        response
            .header("Content-Range")
            .and_then(|h| h.rsplit('/').next())
            .and_then(|n| n.parse::<u64>().ok())
    } else {
        response
            .header("Content-Length")
            .and_then(|value| value.parse::<u64>().ok())
            .map(|n| n + already)
    };
    let mut reader = response.into_reader();
    let mut file = if append {
        fs::OpenOptions::new().append(true).open(tmp)?
    } else {
        File::create(tmp)?
    };
    let mut buf = vec![0_u8; 64 * 1024];
    let mut done = already;
    let meter = ByteProgress::new(progress, url);
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(err)
                if err.to_string().contains("close_notify")
                    || err.to_string().contains("UnexpectedEof") =>
            {
                return Err(ForgeError::Image(format!(
                    "TLS drop while downloading {url} at {}: {err}. Re-run (install curl for resume).",
                    crate::progress::format_bytes(done)
                )));
            }
            Err(err) => return Err(err.into()),
        };
        file.write_all(&buf[..n])?;
        done += n as u64;
        meter.emit(done, total);
    }
    file.sync_all()?;
    Ok(())
}

fn same_local_source(url: &str, dest: &Path) -> bool {
    let local = Path::new(url.strip_prefix("file://").unwrap_or(url));
    if !local.exists() {
        return false;
    }
    match (fs::canonicalize(local), fs::canonicalize(dest)) {
        (Ok(a), Ok(b)) => a == b,
        _ => local == dest,
    }
}

fn utf8(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| ForgeError::Image("non-utf8 path".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::noop;

    #[test]
    fn curl_args_resume_and_retry() {
        let args = curl_args("https://example.test/a.ova", "/tmp/a.part");
        assert!(args.contains(&"-C"));
        assert!(args.contains(&"--retry-all-errors"));
        assert!(args.contains(&"-fL"));
        assert_eq!(*args.last().unwrap(), "https://example.test/a.ova");
    }

    #[test]
    fn cache_skips_network() {
        let dir = std::env::temp_dir().join(format!("forge-dl-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("blob.bin");
        fs::write(&dest, b"already-here").unwrap();
        let got = fetch(
            "http://127.0.0.1:1/must-not-connect",
            &dest,
            Policy::Cache,
            &noop,
        )
        .expect("cache hit");
        assert_eq!(fs::read(&got).unwrap(), b"already-here");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn refresh_overwrites_from_file() {
        let dir = std::env::temp_dir().join(format!("forge-dl-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("blob.bin");
        let src = dir.join("src.bin");
        fs::write(&dest, b"old").unwrap();
        fs::write(&src, b"new").unwrap();
        fetch(&src.to_string_lossy(), &dest, Policy::Refresh, &noop).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"new");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn verified_rejects_stale_cache_then_copies() {
        let dir = std::env::temp_dir().join(format!("forge-dl-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("blob.bin");
        let src = dir.join("good.bin");
        fs::write(&dest, b"stale").unwrap();
        fs::write(&src, b"good").unwrap();
        fetch_verified(&src.to_string_lossy(), &dest, &noop, |path| {
            let bytes = fs::read(path).unwrap();
            if bytes == b"good" {
                Ok(())
            } else {
                Err(ForgeError::Verify("not good".into()))
            }
        })
        .unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"good");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn verified_same_path_failed_check_keeps_file() {
        let dir = std::env::temp_dir().join(format!("forge-dl-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("only.bin");
        fs::write(&dest, b"only-copy").unwrap();
        let err = fetch_verified(&dest.to_string_lossy(), &dest, &noop, |_| {
            Err(ForgeError::Verify("nope".into()))
        })
        .unwrap_err();
        assert!(err.to_string().contains("nope"));
        assert_eq!(fs::read(&dest).unwrap(), b"only-copy");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn verified_keeps_good_cache() {
        let dir = std::env::temp_dir().join(format!("forge-dl-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("blob.bin");
        fs::write(&dest, b"good").unwrap();
        fetch_verified(
            "http://127.0.0.1:1/must-not-connect",
            &dest,
            &noop,
            |path| {
                if fs::read(path).unwrap() == b"good" {
                    Ok(())
                } else {
                    Err(ForgeError::Verify("bad".into()))
                }
            },
        )
        .unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"good");
        let _ = fs::remove_dir_all(dir);
    }
}

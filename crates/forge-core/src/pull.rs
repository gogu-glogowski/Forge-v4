use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cmd;
use crate::error::{ForgeError, Result};
use crate::hash::{self, checksum_for};
use crate::ownership::{BaseProof, write_json_atomic};
use crate::paths::{ForgePaths, chmod};
use crate::profile::{
    Profile, TSURUGI_KEY_ASC, TSURUGI_KEY_FPR, TSURUGI_OVA_NAME, TSURUGI_OVA_URL, TSURUGI_SUMS_URL,
};
use crate::progress::{self, ByteProgress, Progress};
use crate::verify;
use crate::virt;

pub fn pull(paths: &ForgePaths, profile: Profile, progress: &Progress) -> Result<PathBuf> {
    profile.require_engine()?;
    paths.ensure_user()?;
    match profile {
        Profile::Tsurugi => pull_tsurugi(paths, progress),
        _ => unreachable!("require_engine"),
    }
}

fn pull_tsurugi(paths: &ForgePaths, progress: &Progress) -> Result<PathBuf> {
    let dir = paths.cache_dir(Profile::Tsurugi);
    fs::create_dir_all(&dir)?;
    progress::message(progress, "Fetching Tsurugi signed SHA512 hashes");
    let sums_path = download_to(
        TSURUGI_SUMS_URL,
        &dir.join("signed_hashes.sha512"),
        progress,
    )?;
    let keyring =
        verify::import_and_pin(paths, "tsurugi", TSURUGI_KEY_ASC, TSURUGI_KEY_FPR, progress)?;
    let payload = verify::verify_clearsign(&keyring, &sums_path, progress)?;
    let (artifact, expected) = checksum_for(&payload, |name| {
        name.ends_with(".ova") && name.contains("tsurugi_linux")
    })
    .ok_or_else(|| ForgeError::Image("signed hashes have no tsurugi_linux *.ova".to_owned()))?;
    let url = ova_url(&artifact);
    progress::message(progress, format!("Fetching {artifact}"));
    let ova = download_to(&url, &dir.join(&artifact), progress)?;
    progress::message(progress, "Hashing OVA (SHA-512)");
    let actual = hash::sha512_file(&ova, progress)?;
    if actual != expected {
        return Err(ForgeError::Verify(format!(
            "Tsurugi OVA checksum mismatch (expected {expected}, got {actual})"
        )));
    }
    let qcow = convert_ova_to_qcow2(&ova, &dir, progress)?;
    install_base(
        paths,
        Profile::Tsurugi,
        &qcow,
        BaseProof {
            profile: Profile::Tsurugi.id().to_owned(),
            source_url: url,
            artifact,
            upstream_checksum: actual,
            upstream_kind: "sha512-openpgp-clearsign".to_owned(),
            base_digest: String::new(),
            base_path: String::new(),
        },
        progress,
    )
}

fn ova_url(artifact: &str) -> String {
    if let Ok(url) = std::env::var("FORGE_TSURUGI_OVA_URL") {
        return url;
    }
    if artifact == TSURUGI_OVA_NAME {
        TSURUGI_OVA_URL.to_owned()
    } else {
        format!("https://ftp.nluug.nl/os/Linux/distr/tsurugi/01.Tsurugi_Linux_%5bLAB%5d/{artifact}")
    }
}

pub fn convert_ova_to_qcow2(ova: &Path, work: &Path, progress: &Progress) -> Result<PathBuf> {
    let extract = work.join("ova-extract");
    let _ = fs::remove_dir_all(&extract);
    fs::create_dir_all(&extract)?;
    progress::message(progress, format!("Extracting {}", ova.display()));
    cmd::run("tar", &["-xf", path(ova)?, "-C", path(&extract)?])?;
    let disk = find_ova_disk(&extract)?;
    let qcow = work.join("disk.qcow2");
    progress::message(
        progress,
        format!(
            "Converting {} → qcow2 (NIC stripped at domain XML)",
            disk.display()
        ),
    );
    virt::qemu_img_convert(&disk, &qcow)?;
    let _ = fs::remove_dir_all(&extract);
    Ok(qcow)
}

fn find_ova_disk(root: &Path) -> Result<PathBuf> {
    let mut found = Vec::new();
    walk_disks(root, &mut found)?;
    found.sort();
    found
        .into_iter()
        .next()
        .ok_or_else(|| ForgeError::Image("OVA contains no vmdk/vdi/qcow2/vhd disk".to_owned()))
}

fn walk_disks(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_disks(&path, out)?;
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(
            ext.as_str(),
            "vmdk" | "vdi" | "qcow2" | "vhd" | "vhdx" | "img"
        ) {
            out.push(path);
        }
    }
    Ok(())
}

pub fn install_base(
    paths: &ForgePaths,
    profile: Profile,
    src: &Path,
    mut proof: BaseProof,
    progress: &Progress,
) -> Result<PathBuf> {
    progress::message(progress, "Hashing canonical base qcow2 (SHA-256)");
    let digest = format!("sha256:{}", hash::sha256_file(src, progress)?);
    let dest = paths.base_qcow2(profile);
    if let Some(parent) = dest.parent() {
        if paths.privileged_bases {
            cmd::sudo(&["mkdir", "-p", path(parent)?])?;
        } else {
            fs::create_dir_all(parent)?;
        }
    }
    if dest.exists() && paths.privileged_bases {
        let _ = cmd::sudo(&["chattr", "-i", path(&dest)?]);
    }
    if paths.privileged_bases {
        progress::message(
            progress,
            format!(
                "Installing immutable base {} (sudo, no NOPASSWD)",
                dest.display()
            ),
        );
        cmd::sudo(&[
            "install",
            "-o",
            "root",
            "-g",
            "qemu",
            "-m",
            "0440",
            path(src)?,
            path(&dest)?,
        ])?;
        cmd::sudo(&["chattr", "+i", path(&dest)?])?;
    } else {
        fs::copy(src, &dest)?;
        chmod(&dest, 0o440)?;
        let _ = cmd::command("chattr")
            .args(["+i", path(&dest)?])
            .stderr(std::process::Stdio::null())
            .status();
    }
    proof.base_digest = digest;
    proof.base_path = dest.display().to_string();
    write_json_atomic(&paths.proof_file(profile), &proof)?;
    progress::message(progress, format!("Base ready: {}", dest.display()));
    Ok(dest)
}

pub fn base_ready(paths: &ForgePaths, profile: Profile) -> bool {
    paths.base_qcow2(profile).is_file() && paths.proof_file(profile).is_file()
}

pub fn expected_digest(paths: &ForgePaths, profile: Profile) -> Result<String> {
    let proof: crate::ownership::BaseProof =
        crate::ownership::read_json(&paths.proof_file(profile))?;
    Ok(proof.base_digest)
}

pub fn verify_base_digest(
    paths: &ForgePaths,
    profile: Profile,
    progress: &Progress,
) -> Result<String> {
    let expected = expected_digest(paths, profile)?;
    let actual = format!(
        "sha256:{}",
        hash::sha256_file(&paths.base_qcow2(profile), progress)?
    );
    if actual != expected {
        return Err(ForgeError::Verify(format!(
            "base {} digest mismatch (recorded {expected}, now {actual}) — refusing",
            profile.id()
        )));
    }
    Ok(actual)
}

fn download_to(url: &str, dest: &Path, progress: &Progress) -> Result<PathBuf> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(local) = url.strip_prefix("file://") {
        fs::copy(local, dest)?;
        return Ok(dest.to_path_buf());
    }
    if Path::new(url).exists() {
        fs::copy(url, dest)?;
        return Ok(dest.to_path_buf());
    }
    progress::message(progress, format!("GET {url}"));
    let tmp = dest.with_extension("part");
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(30))
        .timeout_read(Duration::from_secs(6 * 60 * 60))
        .timeout_write(Duration::from_secs(60))
        .user_agent("forge/4.0")
        .build();
    let response = agent
        .get(url)
        .call()
        .map_err(|error| ForgeError::Image(format!("download {url}: {error}")))?;
    let total = response
        .header("Content-Length")
        .and_then(|value| value.parse::<u64>().ok());
    let mut reader = response.into_reader();
    let mut file = File::create(&tmp)?;
    let mut buf = vec![0_u8; 64 * 1024];
    let mut done = 0_u64;
    let meter = ByteProgress::new(progress, url);
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        done += n as u64;
        meter.emit(done, total);
    }
    file.sync_all()?;
    fs::rename(&tmp, dest)?;
    Ok(dest.to_path_buf())
}

fn path(p: &Path) -> Result<&str> {
    p.to_str()
        .ok_or_else(|| ForgeError::Image("non-utf8 path".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::noop;

    #[test]
    fn ova_convert_roundtrip() {
        let dir = std::env::temp_dir().join(format!("forge-ova-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let raw = dir.join("disk.raw");
        cmd::run(
            "qemu-img",
            &["create", "-f", "raw", raw.to_str().unwrap(), "1M"],
        )
        .unwrap();
        let vmdk = dir.join("disk.vmdk");
        cmd::run(
            "qemu-img",
            &[
                "convert",
                "-O",
                "vmdk",
                raw.to_str().unwrap(),
                vmdk.to_str().unwrap(),
            ],
        )
        .unwrap();
        let ova = dir.join("tiny.ova");
        cmd::run(
            "tar",
            &[
                "-cf",
                ova.to_str().unwrap(),
                "-C",
                dir.to_str().unwrap(),
                "disk.vmdk",
            ],
        )
        .unwrap();
        let qcow = convert_ova_to_qcow2(&ova, &dir, &noop).expect("convert");
        assert!(qcow.is_file());
        let info = cmd::run_checked("qemu-img", &["info", qcow.to_str().unwrap()]).unwrap();
        assert!(info.contains("qcow2"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn install_base_unprivileged_and_digest() {
        let dir = std::env::temp_dir().join(format!("forge-base-{}", uuid::Uuid::new_v4()));
        let paths = ForgePaths::under(dir.clone(), false);
        paths.ensure_user().unwrap();
        let src = dir.join("src.qcow2");
        cmd::run(
            "qemu-img",
            &["create", "-f", "qcow2", src.to_str().unwrap(), "4M"],
        )
        .unwrap();
        install_base(
            &paths,
            Profile::Tsurugi,
            &src,
            BaseProof {
                profile: "tsurugi".into(),
                source_url: "test".into(),
                artifact: "src.qcow2".into(),
                upstream_checksum: "x".into(),
                upstream_kind: "test".into(),
                base_digest: String::new(),
                base_path: String::new(),
            },
            &noop,
        )
        .unwrap();
        assert!(base_ready(&paths, Profile::Tsurugi));
        verify_base_digest(&paths, Profile::Tsurugi, &noop).unwrap();
        let _ = fs::remove_dir_all(dir);
    }
}

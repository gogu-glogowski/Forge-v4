use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cmd;
use crate::error::{ForgeError, Result};
use crate::hash::{self, checksum_for};
use crate::ownership::{BaseProof, ExtraDisk, write_json_atomic};
use crate::paths::{ForgePaths, chmod};
use crate::profile::{
    KALI_IMAGE_DIR, KALI_KEY_ASC, KALI_KEY_FPR, KALI_SUMS_SIG_URL, KALI_SUMS_URL, Profile,
    SIFT_PAGE_URL, SIFT_PUBLISHED_SHA256, TSURUGI_KEY_ASC, TSURUGI_KEY_FPR, TSURUGI_OVA_NAME,
    TSURUGI_OVA_URL, TSURUGI_SUMS_URL, WHONIX_BUNDLE, WHONIX_BUNDLE_URL, WHONIX_KEY_ASC,
    WHONIX_KEY_FPR, WHONIX_RELEASE, WHONIX_SIG_URL, WHONIX_WS_NAME,
};
use crate::progress::{self, ByteProgress, Progress};
use crate::verify;
use crate::virt;

pub fn pull(paths: &ForgePaths, profile: Profile, progress: &Progress) -> Result<PathBuf> {
    profile.require_engine()?;
    paths.ensure_user()?;
    match profile {
        Profile::Tsurugi => pull_tsurugi(paths, progress),
        Profile::Kali => pull_kali(paths, progress),
        Profile::Whonix => pull_whonix(paths, progress),
        Profile::Sift => pull_sift(paths, progress),
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
            extra: Vec::new(),
        },
        progress,
    )
}

fn pull_kali(paths: &ForgePaths, progress: &Progress) -> Result<PathBuf> {
    let dir = paths.cache_dir(Profile::Kali);
    fs::create_dir_all(&dir)?;
    progress::message(progress, "Fetching Kali SHA256SUMS and signature");
    let sums_url =
        std::env::var("FORGE_KALI_SUMS_URL").unwrap_or_else(|_| KALI_SUMS_URL.to_owned());
    let sig_url =
        std::env::var("FORGE_KALI_SUMS_SIG_URL").unwrap_or_else(|_| KALI_SUMS_SIG_URL.to_owned());
    let sums_path = download_to(&sums_url, &dir.join("SHA256SUMS"), progress)?;
    let sig_path = download_to(&sig_url, &dir.join("SHA256SUMS.gpg"), progress)?;
    let homedir = verify::import_and_pin(paths, "kali", KALI_KEY_ASC, KALI_KEY_FPR, progress)?;
    verify::verify_detached(&homedir, &sig_path, &sums_path, progress)?;
    let sums_text = fs::read_to_string(&sums_path)?;
    let (artifact, expected) = checksum_for(&sums_text, |name| {
        name.contains("qemu-amd64") && name.ends_with(".7z") && !name.contains("torrent")
    })
    .ok_or_else(|| ForgeError::Image("Kali SHA256SUMS has no *-qemu-amd64.7z".to_owned()))?;
    let url = kali_archive_url(&artifact);
    progress::message(progress, format!("Fetching {artifact}"));
    let archive = download_to(&url, &dir.join(&artifact), progress)?;
    progress::message(progress, "Hashing Kali archive (SHA-256)");
    let actual = hash::sha256_file(&archive, progress)?;
    if actual != expected {
        return Err(ForgeError::Verify(format!(
            "Kali archive checksum mismatch (expected {expected}, got {actual})"
        )));
    }
    let qcow = extract_7z_qcow2(&archive, &dir, progress)?;
    install_base(
        paths,
        Profile::Kali,
        &qcow,
        BaseProof {
            profile: Profile::Kali.id().to_owned(),
            source_url: url,
            artifact,
            upstream_checksum: actual,
            upstream_kind: "sha256-openpgp-detached".to_owned(),
            base_digest: String::new(),
            base_path: String::new(),
            extra: Vec::new(),
        },
        progress,
    )
}

fn pull_whonix(paths: &ForgePaths, progress: &Progress) -> Result<PathBuf> {
    let dir = paths.cache_dir(Profile::Whonix);
    fs::create_dir_all(&dir)?;
    let bundle_url =
        std::env::var("FORGE_WHONIX_BUNDLE_URL").unwrap_or_else(|_| WHONIX_BUNDLE_URL.to_owned());
    let sig_url =
        std::env::var("FORGE_WHONIX_SIG_URL").unwrap_or_else(|_| WHONIX_SIG_URL.to_owned());
    progress::message(
        progress,
        format!("Fetching Whonix {WHONIX_RELEASE} libvirt bundle"),
    );
    let bundle = download_to(&bundle_url, &dir.join(WHONIX_BUNDLE), progress)?;
    let sig = download_to(
        &sig_url,
        &dir.join(format!("{WHONIX_BUNDLE}.asc")),
        progress,
    )?;
    let homedir =
        verify::import_and_pin(paths, "whonix", WHONIX_KEY_ASC, WHONIX_KEY_FPR, progress)?;
    verify::verify_detached(&homedir, &sig, &bundle, progress)?;
    let (gateway, workstation) = extract_whonix_bundle(&bundle, &dir, progress)?;
    let (gw_canon, ws_canon) = paths.whonix_bases();
    progress::message(progress, "Installing immutable Whonix bases");
    install_immutable(paths, &gateway, &gw_canon)?;
    install_immutable(paths, &workstation, &ws_canon)?;
    progress::message(progress, "Hashing Whonix bases (SHA-256)");
    let gw_digest = format!("sha256:{}", hash::sha256_file(&gw_canon, progress)?);
    let ws_digest = format!("sha256:{}", hash::sha256_file(&ws_canon, progress)?);
    let proof = BaseProof {
        profile: Profile::Whonix.id().to_owned(),
        source_url: bundle_url,
        artifact: WHONIX_BUNDLE.to_owned(),
        upstream_checksum: hash::sha256_file(&bundle, progress)?,
        upstream_kind: "openpgp-detached".to_owned(),
        base_digest: gw_digest,
        base_path: gw_canon.display().to_string(),
        extra: vec![ExtraDisk {
            name: WHONIX_WS_NAME.to_owned(),
            path: ws_canon.display().to_string(),
            digest: ws_digest,
        }],
    };
    write_json_atomic(&paths.proof_file(Profile::Whonix), &proof)?;
    progress::message(
        progress,
        format!(
            "Bases ready: {} + {}",
            gw_canon.display(),
            ws_canon.display()
        ),
    );
    Ok(gw_canon)
}

pub fn extract_whonix_bundle(
    bundle: &Path,
    work: &Path,
    progress: &Progress,
) -> Result<(PathBuf, PathBuf)> {
    let extract = work.join("whonix-extract");
    let _ = fs::remove_dir_all(&extract);
    fs::create_dir_all(&extract)?;
    progress::message(progress, format!("Extracting {}", bundle.display()));
    cmd::run("tar", &["-xJf", path(bundle)?, "-C", path(&extract)?])?;
    let mut gateway = None;
    let mut workstation = None;
    collect_whonix_qcow2(&extract, &mut gateway, &mut workstation)?;
    let gateway = gateway
        .ok_or_else(|| ForgeError::Image("Whonix bundle has no Gateway qcow2".to_owned()))?;
    let workstation = workstation
        .ok_or_else(|| ForgeError::Image("Whonix bundle has no Workstation qcow2".to_owned()))?;
    Ok((gateway, workstation))
}

fn collect_whonix_qcow2(
    dir: &Path,
    gateway: &mut Option<PathBuf>,
    workstation: &mut Option<PathBuf>,
) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_whonix_qcow2(&path, gateway, workstation)?;
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !name.ends_with(".qcow2") {
            continue;
        }
        if name.contains("gateway") {
            *gateway = Some(path);
        } else if name.contains("workstation") {
            *workstation = Some(path);
        }
    }
    Ok(())
}

fn pull_sift(paths: &ForgePaths, progress: &Progress) -> Result<PathBuf> {
    let dir = paths.cache_dir(Profile::Sift);
    fs::create_dir_all(&dir)?;
    let expected = sift_expected_sha256(paths, progress)?;
    let source = find_sift_ova(&dir)?;
    progress::message(progress, format!("Using SIFT OVA {source}"));
    let ova = if source.starts_with("http://")
        || source.starts_with("https://")
        || source.starts_with("file://")
    {
        download_to(&source, &dir.join("sift.ova"), progress)?
    } else {
        let dest = dir.join(
            Path::new(&source)
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("sift.ova")),
        );
        if dest != Path::new(&source) {
            fs::copy(&source, &dest)?;
        }
        dest
    };
    progress::message(progress, "Hashing SIFT OVA (SHA-256 from SANS page)");
    let actual = hash::sha256_file(&ova, progress)?;
    if actual != expected {
        return Err(ForgeError::Verify(format!(
            "SIFT OVA checksum mismatch (SANS sha256 {expected}, got {actual})"
        )));
    }
    let qcow = convert_ova_to_qcow2(&ova, &dir, progress)?;
    install_base(
        paths,
        Profile::Sift,
        &qcow,
        BaseProof {
            profile: Profile::Sift.id().to_owned(),
            source_url: source,
            artifact: ova
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("sift.ova")
                .to_owned(),
            upstream_checksum: actual,
            upstream_kind: "sans-page-sha256".to_owned(),
            base_digest: String::new(),
            base_path: String::new(),
            extra: Vec::new(),
        },
        progress,
    )
}

fn sift_expected_sha256(paths: &ForgePaths, progress: &Progress) -> Result<String> {
    if let Ok(forced) = std::env::var("FORGE_SIFT_SHA256") {
        let hex = forced.trim().to_ascii_lowercase();
        if hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Ok(hex);
        }
        return Err(ForgeError::Verify(
            "FORGE_SIFT_SHA256 must be 64 hex chars".to_owned(),
        ));
    }
    let page_url = std::env::var("FORGE_SIFT_PAGE").unwrap_or_else(|_| SIFT_PAGE_URL.to_owned());
    progress::message(progress, format!("Reading SIFT SHA-256 from {page_url}"));
    let cache = paths.cache_dir(Profile::Sift).join("sift-page.html");
    let page = download_to(&page_url, &cache, progress)?;
    let html = fs::read_to_string(&page)?;
    hash::sift_sha256_from_page(&html).ok_or_else(|| {
        ForgeError::Verify(format!(
            "could not parse sha256 from {page_url} (published pin {SIFT_PUBLISHED_SHA256})"
        ))
    })
}

fn find_sift_ova(cache: &Path) -> Result<String> {
    if let Ok(src) = std::env::var("FORGE_SIFT_OVA") {
        let src = src.trim();
        if !src.is_empty() {
            return Ok(src.to_owned());
        }
    }
    let mut ovas = Vec::new();
    if cache.is_dir() {
        for entry in fs::read_dir(cache)? {
            let path = entry?.path();
            if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("ova"))
            {
                ovas.push(path);
            }
        }
    }
    match ovas.len() {
        1 => Ok(ovas[0].display().to_string()),
        _ => Err(ForgeError::Image(
            "SIFT OVA is behind SANS Portal login (no mirrors). After login:\n  FORGE_SIFT_OVA=/path/to/sift.ova forge pull sift\nOr drop exactly one .ova in the SIFT cache dir."
                .to_owned(),
        )),
    }
}

fn kali_archive_url(artifact: &str) -> String {
    if let Ok(url) = std::env::var("FORGE_KALI_ARCHIVE_URL") {
        return url;
    }
    format!("{KALI_IMAGE_DIR}/{artifact}")
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

pub fn extract_7z_qcow2(archive: &Path, work: &Path, progress: &Progress) -> Result<PathBuf> {
    let seven = cmd::first_existing(&["7z", "7zz", "7za"])
        .ok_or_else(|| ForgeError::Host("7z not found. sudo dnf install 7zip".to_owned()))?;
    let extract = work.join("7z-extract");
    let _ = fs::remove_dir_all(&extract);
    fs::create_dir_all(&extract)?;
    progress::message(progress, format!("Extracting {}", archive.display()));
    cmd::run(
        &seven,
        &[
            "x",
            "-y",
            &format!("-o{}", extract.display()),
            path(archive)?,
        ],
    )?;
    let mut found = Vec::new();
    find_qcow2(&extract, &mut found)?;
    if found.len() != 1 {
        return Err(ForgeError::Image(format!(
            "Kali archive must contain exactly one qcow2, found {}",
            found.len()
        )));
    }
    let dest = work.join("disk.qcow2");
    fs::copy(&found[0], &dest)?;
    let _ = fs::remove_dir_all(&extract);
    Ok(dest)
}

fn find_qcow2(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            find_qcow2(&path, out)?;
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("qcow2"))
        {
            out.push(path);
        }
    }
    Ok(())
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
    install_immutable(paths, src, &dest)?;
    proof.base_digest = digest;
    proof.base_path = dest.display().to_string();
    write_json_atomic(&paths.proof_file(profile), &proof)?;
    progress::message(progress, format!("Base ready: {}", dest.display()));
    Ok(dest)
}

fn install_immutable(paths: &ForgePaths, src: &Path, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        if paths.privileged_bases {
            cmd::sudo(&["mkdir", "-p", path(parent)?])?;
        } else {
            fs::create_dir_all(parent)?;
        }
    }
    if dest.exists() && paths.privileged_bases {
        let _ = cmd::sudo(&["chattr", "-i", path(dest)?]);
    }
    if paths.privileged_bases {
        cmd::sudo(&[
            "install",
            "-o",
            "root",
            "-g",
            "qemu",
            "-m",
            "0440",
            path(src)?,
            path(dest)?,
        ])?;
        cmd::sudo(&["chattr", "+i", path(dest)?])?;
    } else {
        fs::copy(src, dest)?;
        chmod(dest, 0o440)?;
        let _ = cmd::command("chattr")
            .args(["+i", path(dest)?])
            .stderr(std::process::Stdio::null())
            .status();
    }
    Ok(())
}

pub fn base_ready(paths: &ForgePaths, profile: Profile) -> bool {
    if !paths.proof_file(profile).is_file() {
        return false;
    }
    if profile == Profile::Whonix {
        let (gw, ws) = paths.whonix_bases();
        return gw.is_file() && ws.is_file();
    }
    paths.base_qcow2(profile).is_file()
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
    let proof: BaseProof = crate::ownership::read_json(&paths.proof_file(profile))?;
    verify_file_digest(Path::new(&proof.base_path), &proof.base_digest, progress)?;
    for extra in &proof.extra {
        verify_file_digest(Path::new(&extra.path), &extra.digest, progress)?;
    }
    Ok(proof.base_digest)
}

pub fn verify_file_digest(path: &Path, expected: &str, progress: &Progress) -> Result<String> {
    let actual = format!("sha256:{}", hash::sha256_file(path, progress)?);
    if actual != expected {
        return Err(ForgeError::Verify(format!(
            "base {} digest mismatch (recorded {expected}, now {actual}) — refusing",
            path.display()
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
                extra: Vec::new(),
            },
            &noop,
        )
        .unwrap();
        assert!(base_ready(&paths, Profile::Tsurugi));
        verify_base_digest(&paths, Profile::Tsurugi, &noop).unwrap();
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn seven_zip_extracts_single_qcow2() {
        let dir = std::env::temp_dir().join(format!("forge-7z-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let qcow = dir.join("kali-disk.qcow2");
        cmd::run(
            "qemu-img",
            &["create", "-f", "qcow2", qcow.to_str().unwrap(), "1M"],
        )
        .unwrap();
        let archive = dir.join("kali-linux-2026.2-qemu-amd64.7z");
        cmd::run(
            "7z",
            &["a", "-y", archive.to_str().unwrap(), qcow.to_str().unwrap()],
        )
        .unwrap();
        let out = extract_7z_qcow2(&archive, &dir, &noop).expect("extract");
        assert!(out.is_file());
        let info = cmd::run_checked("qemu-img", &["info", out.to_str().unwrap()]).unwrap();
        assert!(info.contains("qcow2"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn kali_sums_pick_qemu_amd64_7z() {
        let sums = "\
deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef  kali-linux-2026.2-qemu-amd64.7z.torrent
cafebabecafebabecafebabecafebabecafebabecafebabecafebabecafebabe  kali-linux-2026.2-qemu-amd64.7z
";
        let (name, hash) = checksum_for(sums, |n| {
            n.contains("qemu-amd64") && n.ends_with(".7z") && !n.contains("torrent")
        })
        .expect("qemu 7z");
        assert_eq!(name, "kali-linux-2026.2-qemu-amd64.7z");
        assert_eq!(
            hash,
            "cafebabecafebabecafebabecafebabecafebabecafebabecafebabecafebabe"
        );
    }

    #[test]
    fn whonix_bundle_extracts_pair() {
        let dir = std::env::temp_dir().join(format!("forge-wx-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let gw = dir.join("Whonix-Gateway-LXQt.qcow2");
        let ws = dir.join("Whonix-Workstation-LXQt.qcow2");
        cmd::run(
            "qemu-img",
            &["create", "-f", "qcow2", gw.to_str().unwrap(), "1M"],
        )
        .unwrap();
        cmd::run(
            "qemu-img",
            &["create", "-f", "qcow2", ws.to_str().unwrap(), "1M"],
        )
        .unwrap();
        let bundle = dir.join("Whonix-LXQt.qcow2.libvirt.xz");
        cmd::run(
            "tar",
            &[
                "-cJf",
                bundle.to_str().unwrap(),
                "-C",
                dir.to_str().unwrap(),
                "Whonix-Gateway-LXQt.qcow2",
                "Whonix-Workstation-LXQt.qcow2",
            ],
        )
        .unwrap();
        let (g, w) = extract_whonix_bundle(&bundle, &dir, &noop).expect("extract");
        assert!(g.to_string_lossy().to_ascii_lowercase().contains("gateway"));
        assert!(
            w.to_string_lossy()
                .to_ascii_lowercase()
                .contains("workstation")
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn sift_finds_single_ova_in_cache() {
        let dir = std::env::temp_dir().join(format!("forge-sift-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let ova = dir.join("SIFT-Workstation.ova");
        fs::write(&ova, b"not-a-real-ova").unwrap();
        let found = find_sift_ova(&dir).unwrap();
        assert!(found.ends_with("SIFT-Workstation.ova"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn sift_refuses_without_ova() {
        let dir = std::env::temp_dir().join(format!("forge-sift-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let err = find_sift_ova(&dir).unwrap_err();
        assert!(err.to_string().contains("SANS Portal"));
        let _ = fs::remove_dir_all(dir);
    }
}

use std::fs;

use crate::error::{ForgeError, Result};
use crate::ownership::{self, Ownership};
use crate::paths::ForgePaths;
use crate::profile::{Profile, SYSTEM_URI};
use crate::progress::{self, Progress};
use crate::pull;
use crate::role::{Role, VmPower};
use crate::virt;
use crate::xml::{self, DomainSpec};

#[derive(Debug, Clone)]
pub struct Forge {
    pub paths: ForgePaths,
    pub uri: String,
}

#[derive(Debug, Clone)]
pub struct Created {
    pub name: String,
    pub uuid: String,
    pub xml: String,
}

#[derive(Debug)]
pub struct VmStatus {
    pub ownership: Ownership,
    pub power: VmPower,
    pub role_ok: Result<()>,
}

#[derive(Debug, Clone)]
pub struct InventoryRow {
    pub profile: Profile,
    pub image: &'static str,
    pub vms: Vec<String>,
}

impl Forge {
    pub fn open() -> Result<Self> {
        let paths = ForgePaths::discover();
        paths.ensure_user()?;
        let uri = virt::uri()?;
        Ok(Self { paths, uri })
    }

    pub fn open_paths(paths: ForgePaths) -> Result<Self> {
        paths.ensure_user()?;
        Ok(Self {
            paths,
            uri: SYSTEM_URI.to_owned(),
        })
    }

    pub fn pull(&self, profile: Profile, progress: &Progress) -> Result<()> {
        pull::pull(&self.paths, profile, progress)?;
        Ok(())
    }

    pub fn create(
        &self,
        profile: Profile,
        name: Option<&str>,
        dry_run: bool,
        progress: &Progress,
    ) -> Result<Created> {
        profile.require_engine()?;
        let name = name
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("tsurugi");
        validate_vm_name(name)?;
        if !pull::base_ready(&self.paths, profile) {
            return Err(ForgeError::NotFound(format!(
                "no verified base for {} — run: forge pull {}",
                profile.id(),
                profile.id()
            )));
        }
        let digest = pull::verify_base_digest(&self.paths, profile, progress)?;
        let overlay = self.paths.overlay_qcow2(name);
        let base = self.paths.base_qcow2(profile);
        let uuid = uuid::Uuid::new_v4().to_string();
        let spec = DomainSpec {
            name,
            uuid: &uuid,
            profile: profile.id(),
            role: Role::Isolated,
            overlay: &overlay.to_string_lossy(),
            base_digest: &digest,
            memory_mib: profile.memory_mib(),
            vcpus: profile.vcpus(),
        };
        let xml = xml::domain_xml(&spec);
        xml::check_role(&xml, Role::Isolated)?;
        if dry_run {
            return Ok(Created {
                name: name.to_owned(),
                uuid,
                xml,
            });
        }
        if virt::domain_exists(&self.uri, name)? {
            return Err(ForgeError::AlreadyExists(format!(
                "libvirt domain {name} already exists"
            )));
        }
        if overlay.exists() {
            return Err(ForgeError::AlreadyExists(format!(
                "overlay {} already exists",
                overlay.display()
            )));
        }
        if self.paths.ownership_file(name).exists() {
            return Err(ForgeError::AlreadyExists(format!(
                "Forge already owns VM {name}"
            )));
        }
        let _ = virt::ensure_vms_pool(&self.uri, &self.paths.vms);
        if !self.paths.vms.exists() {
            fs::create_dir_all(&self.paths.vms)?;
        }
        progress::message(progress, format!("Creating overlay {}", overlay.display()));
        virt::qemu_img_create_overlay(&base, &overlay)?;
        let ownership = Ownership {
            name: name.to_owned(),
            uuid: uuid.clone(),
            profile: profile.id().to_owned(),
            role: Role::Isolated.id().to_owned(),
            overlay: overlay.display().to_string(),
            base: base.display().to_string(),
            base_digest: digest,
            libvirt_uri: self.uri.clone(),
        };
        ownership::write_json_atomic(&self.paths.ownership_file(name), &ownership)?;
        virt::define_xml(&self.uri, &xml)?;
        Ok(Created {
            name: name.to_owned(),
            uuid,
            xml,
        })
    }

    pub fn start(&self, name: &str, progress: &Progress) -> Result<()> {
        let own = self.require_owned(name)?;
        let profile = own.profile()?;
        pull::verify_base_digest(&self.paths, profile, progress)?;
        self.assert_backing(&own)?;
        let xml = virt::dumpxml(&self.uri, name)?;
        xml::check_role(&xml, own.role()?)?;
        self.assert_dongle_exclusive(Some(name))?;
        virt::start(&self.uri, name)?;
        Ok(())
    }

    pub fn stop(&self, name: &str, force: bool) -> Result<()> {
        let _ = self.require_owned(name)?;
        if force {
            virt::destroy(&self.uri, name)?;
        } else {
            virt::shutdown(&self.uri, name)?;
        }
        Ok(())
    }

    pub fn status(&self, name: Option<&str>) -> Result<Vec<VmStatus>> {
        match name {
            Some(name) => Ok(vec![self.status_one(name)?]),
            None => {
                let mut rows = Vec::new();
                for name in self.owned_names()? {
                    rows.push(self.status_one(&name)?);
                }
                self.assert_dongle_exclusive(None)?;
                Ok(rows)
            }
        }
    }

    fn status_one(&self, name: &str) -> Result<VmStatus> {
        let ownership = self.require_owned(name)?;
        let power = virt::domstate(&self.uri, name)?;
        let xml = virt::dumpxml(&self.uri, name)?;
        let role_ok = xml::check_role(&xml, ownership.role()?);
        Ok(VmStatus {
            ownership,
            power,
            role_ok,
        })
    }

    pub fn list(&self) -> Result<Vec<InventoryRow>> {
        let owned = self.owned_all()?;
        Ok(Profile::all()
            .into_iter()
            .map(|profile| {
                let image = if pull::base_ready(&self.paths, profile) {
                    "ready"
                } else {
                    "missing"
                };
                let vms = owned
                    .iter()
                    .filter(|o| o.profile == profile.id())
                    .map(|o| o.name.clone())
                    .collect();
                InventoryRow {
                    profile,
                    image,
                    vms,
                }
            })
            .collect())
    }

    pub fn delete(&self, name: &str, dry_run: bool) -> Result<String> {
        let own = self.require_owned(name)?;
        let xml = virt::dumpxml(&self.uri, name).ok();
        if let Some(xml) = &xml {
            if !xml.contains(&own.uuid) || !xml::is_forge_domain(xml) {
                return Err(ForgeError::Ownership(format!(
                    "{name}: libvirt UUID/metadata does not match Forge ownership — refusing delete"
                )));
            }
        } else {
            return Err(ForgeError::Ownership(format!(
                "{name}: no libvirt domain; refusing to guess"
            )));
        }
        let power = virt::domstate(&self.uri, name)?;
        if power.is_active() {
            return Err(ForgeError::Ownership(format!(
                "{name} is {}; stop it first",
                power.as_str()
            )));
        }
        let plan = format!(
            "delete domain {name} uuid {} overlay {} (base {} stays)",
            own.uuid, own.overlay, own.base
        );
        if dry_run {
            return Ok(plan);
        }
        virt::undefine(&self.uri, name)?;
        let overlay = std::path::PathBuf::from(&own.overlay);
        if overlay.exists() {
            fs::remove_file(&overlay)?;
        }
        let _ = fs::remove_file(self.paths.ownership_file(name));
        Ok(plan)
    }

    pub fn clone_vm(&self, src: &str, dst: &str, progress: &Progress) -> Result<Created> {
        validate_vm_name(dst)?;
        let own = self.require_owned(src)?;
        let profile = own.profile()?;
        if profile == Profile::Whonix {
            return Err(ForgeError::InvalidInput(
                "clone of Whonix pair is not 4.0".to_owned(),
            ));
        }
        if !profile.is_isolated() {
            return Err(ForgeError::NotThisCut(
                "this cut clones isolated VMs only".to_owned(),
            ));
        }
        progress::message(progress, format!("Cloning overlay {src} → {dst}"));
        let digest = pull::verify_base_digest(&self.paths, profile, progress)?;
        let src_overlay = std::path::PathBuf::from(&own.overlay);
        let dst_overlay = self.paths.overlay_qcow2(dst);
        if dst_overlay.exists() || virt::domain_exists(&self.uri, dst)? {
            return Err(ForgeError::AlreadyExists(format!("target {dst} exists")));
        }
        fs::copy(&src_overlay, &dst_overlay)?;
        let uuid = uuid::Uuid::new_v4().to_string();
        let spec = DomainSpec {
            name: dst,
            uuid: &uuid,
            profile: profile.id(),
            role: Role::Isolated,
            overlay: &dst_overlay.to_string_lossy(),
            base_digest: &digest,
            memory_mib: profile.memory_mib(),
            vcpus: profile.vcpus(),
        };
        let xml = xml::domain_xml(&spec);
        let ownership = Ownership {
            name: dst.to_owned(),
            uuid: uuid.clone(),
            profile: profile.id().to_owned(),
            role: Role::Isolated.id().to_owned(),
            overlay: dst_overlay.display().to_string(),
            base: own.base.clone(),
            base_digest: digest,
            libvirt_uri: self.uri.clone(),
        };
        ownership::write_json_atomic(&self.paths.ownership_file(dst), &ownership)?;
        virt::define_xml(&self.uri, &xml)?;
        Ok(Created {
            name: dst.to_owned(),
            uuid,
            xml,
        })
    }

    pub fn dump_xml(&self, name: &str) -> Result<String> {
        let own = self.require_owned(name)?;
        let xml = virt::dumpxml(&self.uri, name)?;
        let mut out = xml.clone();
        out.push_str("\n<!-- Forge role check: ");
        match xml::check_role(&xml, own.role()?) {
            Ok(()) => out.push_str("ok -->\n"),
            Err(error) => out.push_str(&format!("{error} -->\n")),
        }
        Ok(out)
    }

    fn require_owned(&self, name: &str) -> Result<Ownership> {
        let path = self.paths.ownership_file(name);
        if !path.is_file() {
            return Err(ForgeError::Ownership(format!(
                "{name} is not a Forge VM (no ownership file)"
            )));
        }
        let own: Ownership = ownership::read_json(&path)?;
        if own.name != name {
            return Err(ForgeError::Ownership(format!(
                "ownership name {} != {name}",
                own.name
            )));
        }
        if own.libvirt_uri != SYSTEM_URI && !own.libvirt_uri.is_empty() {
            return Err(ForgeError::Ownership(format!(
                "{name} recorded URI {} — Forge uses {SYSTEM_URI}",
                own.libvirt_uri
            )));
        }
        Ok(own)
    }

    fn owned_names(&self) -> Result<Vec<String>> {
        Ok(self.owned_all()?.into_iter().map(|o| o.name).collect())
    }

    fn owned_all(&self) -> Result<Vec<Ownership>> {
        let dir = self.paths.ownership_root();
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(own) = ownership::read_json::<Ownership>(&path) {
                out.push(own);
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    fn assert_backing(&self, own: &Ownership) -> Result<()> {
        let overlay = std::path::PathBuf::from(&own.overlay);
        let backing = virt::qemu_img_backing(&overlay)?
            .ok_or_else(|| ForgeError::Verify(format!("{} has no backing file", own.name)))?;
        let base = std::path::PathBuf::from(&own.base);
        let backing_canon = std::fs::canonicalize(&backing).unwrap_or_else(|_| backing.into());
        let base_canon = std::fs::canonicalize(&base).unwrap_or(base);
        if backing_canon != base_canon {
            return Err(ForgeError::Verify(format!(
                "{} overlay backing {} is not the recorded base {}",
                own.name,
                backing_canon.display(),
                base_canon.display()
            )));
        }
        Ok(())
    }

    fn assert_dongle_exclusive(&self, starting: Option<&str>) -> Result<()> {
        let names = virt::list_all_names(&self.uri).unwrap_or_default();
        let mut holders = Vec::new();
        for name in names {
            let Ok(xml) = virt::dumpxml(&self.uri, &name) else {
                continue;
            };
            if xml::inspect(&xml).hostdev_usb > 0 {
                holders.push(name);
            }
        }
        if holders.len() > 1 {
            return Err(ForgeError::Role(format!(
                "dongle B in more than one VM: {}",
                holders.join(", ")
            )));
        }
        let _ = starting;
        Ok(())
    }
}

fn validate_vm_name(name: &str) -> Result<()> {
    if name.len() > 48
        || !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        || name.starts_with('-')
        || name.ends_with('-')
    {
        return Err(ForgeError::InvalidInput(format!(
            "bad VM name '{name}' (ascii lowercase, digits, hyphen)"
        )));
    }
    Ok(())
}

#[must_use]
pub fn format_list(rows: &[InventoryRow]) -> String {
    let mut out = format!("{:<10} {:<12} {}\n", "profile", "image", "vms");
    for row in rows {
        let vms = if row.vms.is_empty() {
            "—".to_owned()
        } else {
            row.vms.join(", ")
        };
        out.push_str(&format!(
            "{:<10} {:<12} {}\n",
            row.profile.id(),
            row.image,
            vms
        ));
    }
    out
}

#[must_use]
pub fn format_status(rows: &[VmStatus]) -> String {
    let mut out = String::new();
    for row in rows {
        out.push_str(&format!(
            "{:<18} {:<10} {:<14} {}",
            row.ownership.name,
            row.ownership.profile,
            row.power.as_str(),
            row.ownership.role
        ));
        match &row.role_ok {
            Ok(()) => out.push_str("  role=ok\n"),
            Err(error) => out.push_str(&format!("  ROLE FAIL: {error}\n")),
        }
    }
    if out.is_empty() {
        out.push_str("No Forge VMs.\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_run_create_isolated_xml() {
        let dir = std::env::temp_dir().join(format!("forge-lab-{}", uuid::Uuid::new_v4()));
        let paths = ForgePaths::under(dir.clone(), false);
        paths.ensure_user().unwrap();
        let src = dir.join("src.qcow2");
        crate::cmd::run(
            "qemu-img",
            &["create", "-f", "qcow2", src.to_str().unwrap(), "4M"],
        )
        .unwrap();
        crate::pull::install_base(
            &paths,
            Profile::Tsurugi,
            &src,
            crate::ownership::BaseProof {
                profile: "tsurugi".into(),
                source_url: "test".into(),
                artifact: "src.qcow2".into(),
                upstream_checksum: "x".into(),
                upstream_kind: "test".into(),
                base_digest: String::new(),
                base_path: String::new(),
            },
            &progress::noop,
        )
        .unwrap();
        let forge = Forge::open_paths(paths).unwrap();
        let created = forge
            .create(Profile::Tsurugi, None, true, &progress::noop)
            .unwrap();
        assert_eq!(created.name, "tsurugi");
        assert!(!created.xml.contains("<interface"));
        xml::check_role(&created.xml, Role::Isolated).unwrap();
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn kali_create_is_not_this_cut() {
        let dir = std::env::temp_dir().join(format!("forge-lab-{}", uuid::Uuid::new_v4()));
        let forge = Forge::open_paths(ForgePaths::under(dir.clone(), false)).unwrap();
        let err = forge
            .create(Profile::Kali, None, true, &progress::noop)
            .unwrap_err();
        assert!(err.to_string().contains("this cut"));
        let _ = fs::remove_dir_all(dir);
    }
}

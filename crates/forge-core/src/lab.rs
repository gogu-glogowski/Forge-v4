use std::fs;
use std::path::Path;

use crate::error::{ForgeError, Result};
use crate::ownership::{self, Ownership};
use crate::paths::ForgePaths;
use crate::profile::{Profile, SYSTEM_URI, WHONIX_GW_NAME, WHONIX_WS_NAME};
use crate::progress::{self, Progress};
use crate::pull;
use crate::role::{Role, VmPower};
use crate::usb::{self, Resolve, UsbId};
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
    ) -> Result<Vec<Created>> {
        profile.require_engine()?;
        if !pull::base_ready(&self.paths, profile) {
            return Err(ForgeError::NotFound(format!(
                "no verified base for {} — run: forge pull {}",
                profile.id(),
                profile.id()
            )));
        }
        pull::verify_base_digest(&self.paths, profile, progress)?;
        if profile == Profile::Whonix {
            if name.is_some() {
                return Err(ForgeError::InvalidInput(
                    "create whonix takes no extra name — it always creates whonix-gateway + whonix-workstation"
                        .to_owned(),
                ));
            }
            return self.create_whonix_pair(dry_run, progress);
        }
        let name = name
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| profile.default_name());
        Ok(vec![self.create_one(
            profile,
            name,
            profile.role(),
            &self.paths.base_qcow2(profile),
            None,
            dry_run,
            progress,
        )?])
    }

    fn create_whonix_pair(&self, dry_run: bool, progress: &Progress) -> Result<Vec<Created>> {
        let proof: crate::ownership::BaseProof =
            crate::ownership::read_json(&self.paths.proof_file(Profile::Whonix))?;
        let ws = proof.extra.first().ok_or_else(|| {
            ForgeError::Ownership("whonix proof has no workstation disk".to_owned())
        })?;
        let gw_digest = proof.base_digest.clone();
        let gw_path = proof.base_path.clone();
        let ws_path = ws.path.clone();
        let ws_digest = ws.digest.clone();
        if !dry_run {
            virt::ensure_whonix_net(&self.uri)?;
        }
        let gw = self.create_one(
            Profile::Whonix,
            WHONIX_GW_NAME,
            Role::WhonixGw,
            Path::new(&gw_path),
            Some((gw_digest.as_str(), Profile::Whonix.memory_mib())),
            dry_run,
            progress,
        )?;
        let ws_vm = self.create_one(
            Profile::Whonix,
            WHONIX_WS_NAME,
            Role::WhonixWs,
            Path::new(&ws_path),
            Some((ws_digest.as_str(), Profile::Whonix.workstation_memory_mib())),
            dry_run,
            progress,
        )?;
        Ok(vec![gw, ws_vm])
    }

    #[allow(clippy::too_many_arguments)]
    fn create_one(
        &self,
        profile: Profile,
        name: &str,
        role: Role,
        base: &Path,
        digest_mem: Option<(&str, u32)>,
        dry_run: bool,
        progress: &Progress,
    ) -> Result<Created> {
        validate_vm_name(name)?;
        let digest = if let Some((d, _)) = digest_mem {
            d.to_owned()
        } else {
            pull::verify_file_digest(
                base,
                &pull::expected_digest(&self.paths, profile)?,
                progress,
            )?
        };
        let memory = digest_mem
            .map(|(_, m)| m)
            .unwrap_or_else(|| profile.memory_mib());
        let overlay = self.paths.overlay_qcow2(name);
        let uuid = uuid::Uuid::new_v4().to_string();
        let spec = DomainSpec {
            name,
            uuid: &uuid,
            profile: profile.id(),
            role,
            overlay: &overlay.to_string_lossy(),
            base_digest: &digest,
            memory_mib: memory,
            vcpus: profile.vcpus(),
        };
        let xml = xml::domain_xml(&spec);
        xml::check_role(&xml, role)?;
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
        virt::qemu_img_create_overlay(base, &overlay)?;
        let ownership = Ownership {
            name: name.to_owned(),
            uuid: uuid.clone(),
            profile: profile.id().to_owned(),
            role: role.id().to_owned(),
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
        pull::verify_file_digest(Path::new(&own.base), &own.base_digest, progress)?;
        self.assert_backing(&own)?;
        let xml = virt::dumpxml(&self.uri, name)?;
        xml::check_role(&xml, own.role()?)?;
        self.assert_dongle_exclusive(Some(name))?;
        if name == WHONIX_WS_NAME {
            let gw = virt::domstate(&self.uri, WHONIX_GW_NAME).unwrap_or(VmPower::Shutoff);
            if !gw.is_active() {
                return Err(ForgeError::Role(
                    "start whonix-gateway before whonix-workstation".to_owned(),
                ));
            }
        }
        virt::start(&self.uri, name)?;
        self.maybe_attach_dongle_b(name, own.role()?, progress)?;
        Ok(())
    }

    pub fn stop(&self, name: &str, force: bool) -> Result<()> {
        let own = self.require_owned(name)?;
        if name == WHONIX_GW_NAME && !force {
            if let Ok(ws) = virt::domstate(&self.uri, WHONIX_WS_NAME) {
                if ws.is_active() {
                    return Err(ForgeError::Role(
                        "stop whonix-workstation before whonix-gateway (or use --force)".to_owned(),
                    ));
                }
            }
        }
        let _ = self.maybe_detach_dongle_b(name, own.role()?);
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
        if !matches!(profile.role(), Role::Isolated | Role::OsintClearnet) {
            return Err(ForgeError::NotThisCut(
                "this cut clones isolated and osint-clearnet VMs only".to_owned(),
            ));
        }
        let role = profile.role();
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
            role,
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
            role: role.id().to_owned(),
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

    fn maybe_attach_dongle_b(&self, name: &str, role: Role, progress: &Progress) -> Result<()> {
        if !matches!(role, Role::WhonixGw | Role::OsintClearnet) {
            return Ok(());
        }
        match usb::resolve_plugged(&self.paths)? {
            Resolve::Plugged(id) => {
                if let Some(other) = self.dongle_holder(Some(id), Some(name))? {
                    progress::message(
                        progress,
                        format!("dongle B {} already in {other}; not stealing", id.display()),
                    );
                    return Ok(());
                }
                progress::message(
                    progress,
                    format!("attaching dongle B {} to {name} (live)", id.display()),
                );
                virt::attach_device_live(&self.uri, name, &id.hostdev_xml())?;
            }
            Resolve::PinnedMissing(id) => {
                progress::message(
                    progress,
                    format!(
                        "dongle B {} not plugged; starting without WAN",
                        id.display()
                    ),
                );
            }
            Resolve::None => {
                progress::message(progress, "dongle B not plugged; starting without WAN");
            }
        }
        Ok(())
    }

    fn maybe_detach_dongle_b(&self, name: &str, role: Role) -> Result<()> {
        if !matches!(role, Role::WhonixGw | Role::OsintClearnet) {
            return Ok(());
        }
        let Ok(xml) = virt::dumpxml(&self.uri, name) else {
            return Ok(());
        };
        for id in xml::inspect(&xml).usb_ids {
            let _ = virt::detach_device_live(&self.uri, name, &id.hostdev_xml());
        }
        Ok(())
    }

    fn dongle_holder(&self, id: Option<UsbId>, except: Option<&str>) -> Result<Option<String>> {
        let names = virt::list_all_names(&self.uri).unwrap_or_default();
        for name in names {
            if except == Some(name.as_str()) {
                continue;
            }
            let Ok(xml) = virt::dumpxml(&self.uri, &name) else {
                continue;
            };
            let facts = xml::inspect(&xml);
            let hit = match id {
                Some(id) => facts.usb_ids.contains(&id),
                None => facts.hostdev_usb > 0,
            };
            if hit {
                return Ok(Some(name));
            }
        }
        Ok(None)
    }

    fn assert_dongle_exclusive(&self, starting: Option<&str>) -> Result<()> {
        let names = virt::list_all_names(&self.uri).unwrap_or_default();
        let mut holders: Vec<(String, String)> = Vec::new();
        for name in names {
            let Ok(xml) = virt::dumpxml(&self.uri, &name) else {
                continue;
            };
            for id in xml::inspect(&xml).usb_ids {
                holders.push((name.clone(), id.display()));
            }
        }
        let _ = starting;
        if holders.len() <= 1 {
            return Ok(());
        }
        let mut by_id: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for (vm, id) in holders {
            by_id.entry(id).or_default().push(vm);
        }
        let clashes: Vec<String> = by_id
            .into_iter()
            .filter(|(_, vms)| vms.len() > 1)
            .map(|(id, vms)| format!("{id} in {}", vms.join("+")))
            .collect();
        if clashes.is_empty() {
            Ok(())
        } else {
            Err(ForgeError::Role(format!(
                "dongle B in more than one VM: {}",
                clashes.join("; ")
            )))
        }
    }

    pub fn usb_report(&self) -> String {
        usb::format_dev_list(&self.paths)
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
                extra: Vec::new(),
            },
            &progress::noop,
        )
        .unwrap();
        let forge = Forge::open_paths(paths).unwrap();
        let created = forge
            .create(Profile::Tsurugi, None, true, &progress::noop)
            .unwrap();
        assert_eq!(created[0].name, "tsurugi");
        assert!(!created[0].xml.contains("<interface"));
        xml::check_role(&created[0].xml, Role::Isolated).unwrap();
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn dry_run_create_kali_osint() {
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
            Profile::Kali,
            &src,
            crate::ownership::BaseProof {
                profile: "kali".into(),
                source_url: "test".into(),
                artifact: "src.qcow2".into(),
                upstream_checksum: "x".into(),
                upstream_kind: "test".into(),
                base_digest: String::new(),
                base_path: String::new(),
                extra: Vec::new(),
            },
            &progress::noop,
        )
        .unwrap();
        let forge = Forge::open_paths(paths).unwrap();
        let created = forge
            .create(Profile::Kali, None, true, &progress::noop)
            .unwrap();
        assert_eq!(created[0].name, "kali");
        assert!(!created[0].xml.contains("<interface"));
        assert!(created[0].xml.contains("osint-clearnet"));
        xml::check_role(&created[0].xml, Role::OsintClearnet).unwrap();
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn dry_run_create_sift_isolated() {
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
            Profile::Sift,
            &src,
            crate::ownership::BaseProof {
                profile: "sift".into(),
                source_url: "test".into(),
                artifact: "src.qcow2".into(),
                upstream_checksum: "x".into(),
                upstream_kind: "test".into(),
                base_digest: String::new(),
                base_path: String::new(),
                extra: Vec::new(),
            },
            &progress::noop,
        )
        .unwrap();
        let forge = Forge::open_paths(paths).unwrap();
        let created = forge
            .create(Profile::Sift, None, true, &progress::noop)
            .unwrap();
        assert_eq!(created[0].name, "sift");
        assert!(!created[0].xml.contains("<interface"));
        xml::check_role(&created[0].xml, Role::Isolated).unwrap();
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn dry_run_create_whonix_pair() {
        let dir = std::env::temp_dir().join(format!("forge-lab-{}", uuid::Uuid::new_v4()));
        let paths = ForgePaths::under(dir.clone(), false);
        paths.ensure_user().unwrap();
        let (gw, ws) = paths.whonix_bases();
        crate::cmd::run(
            "qemu-img",
            &["create", "-f", "qcow2", gw.to_str().unwrap(), "4M"],
        )
        .unwrap();
        crate::cmd::run(
            "qemu-img",
            &["create", "-f", "qcow2", ws.to_str().unwrap(), "4M"],
        )
        .unwrap();
        let gw_digest = format!(
            "sha256:{}",
            crate::hash::sha256_file(&gw, &progress::noop).unwrap()
        );
        let ws_digest = format!(
            "sha256:{}",
            crate::hash::sha256_file(&ws, &progress::noop).unwrap()
        );
        crate::ownership::write_json_atomic(
            &paths.proof_file(Profile::Whonix),
            &crate::ownership::BaseProof {
                profile: "whonix".into(),
                source_url: "test".into(),
                artifact: "bundle".into(),
                upstream_checksum: "x".into(),
                upstream_kind: "test".into(),
                base_digest: gw_digest,
                base_path: gw.display().to_string(),
                extra: vec![crate::ownership::ExtraDisk {
                    name: "whonix-workstation".into(),
                    path: ws.display().to_string(),
                    digest: ws_digest,
                }],
            },
        )
        .unwrap();
        let forge = Forge::open_paths(paths).unwrap();
        let created = forge
            .create(Profile::Whonix, None, true, &progress::noop)
            .unwrap();
        assert_eq!(created.len(), 2);
        assert_eq!(created[0].name, "whonix-gateway");
        assert_eq!(created[1].name, "whonix-workstation");
        xml::check_role(&created[0].xml, Role::WhonixGw).unwrap();
        xml::check_role(&created[1].xml, Role::WhonixWs).unwrap();
        assert!(created[0].xml.contains("forge-whonix"));
        assert!(!created[0].xml.contains("type='user'"));
        let _ = fs::remove_dir_all(dir);
    }
}

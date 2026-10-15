//! Resumable orchestration; the backend keeps libvirt/storage effects testable.
use crate::progress::{self, Progress};
use crate::{ForgeError, Profile, Result};
use std::path::Path;

#[derive(Debug, Default)]
pub struct PrepareReport {
    pub prepared: Vec<Profile>,
    pub skipped: Vec<String>,
}

pub(crate) trait Backend {
    fn setup(&self) -> Result<()>;
    fn base_ready(&self, profile: Profile) -> bool;
    fn has_references(&self, profile: Profile) -> Result<bool>;
    fn pull(&self, profile: Profile, sift_ova: Option<&Path>, progress: &Progress) -> Result<()>;
    fn verify(&self, profile: Profile, progress: &Progress) -> Result<()>;
    fn ensure_vms(&self, profile: Profile, progress: &Progress) -> Result<()>;
}

pub(crate) fn run(
    backend: &impl Backend,
    sift_ova: Option<&Path>,
    progress: &Progress,
) -> Result<PrepareReport> {
    if sift_ova.is_some_and(|path| !path.is_file()) {
        return Err(ForgeError::InvalidInput(
            "--sift-ova must point to an existing local OVA file".to_owned(),
        ));
    }
    progress::message(
        progress,
        "[1/5] Configuring Forge; guests must remain stopped",
    );
    backend.setup()?;
    let mut report = PrepareReport::default();
    for (index, profile) in [
        Profile::Tsurugi,
        Profile::Kali,
        Profile::Whonix,
        Profile::Sift,
    ]
    .into_iter()
    .enumerate()
    {
        progress::message(
            progress,
            format!("[{}/5] Preparing {}", index + 2, profile.id()),
        );
        if backend.base_ready(profile) {
            progress::message(
                progress,
                "Base already present; verify and reuse without downloading or replacing it",
            );
        } else {
            if backend.has_references(profile)? {
                return Err(ForgeError::Verify(format!(
                    "{} base is missing/incomplete but existing VM data references it; refusing to replace the base",
                    profile.id()
                )));
            }
            if profile == Profile::Sift && sift_ova.is_none() {
                report.skipped.push(
                    "SIFT: manual SANS download required; rerun with --sift-ova /path/to/SIFT.ova"
                        .to_owned(),
                );
                progress::message(progress, report.skipped.last().unwrap());
                continue;
            }
            backend.pull(profile, sift_ova, progress)?;
        }
        backend.verify(profile, progress)?;
        backend.ensure_vms(profile, progress)?;
        report.prepared.push(profile);
    }
    progress::message(progress, "Preparation complete. VMs remain powered off.");
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    #[derive(Default)]
    struct Fake {
        ready: RefCell<Vec<Profile>>,
        referenced: Vec<Profile>,
        events: RefCell<Vec<String>>,
        fail_on: Option<Profile>,
    }
    impl Backend for Fake {
        fn setup(&self) -> Result<()> {
            self.events.borrow_mut().push("setup".into());
            Ok(())
        }
        fn base_ready(&self, p: Profile) -> bool {
            self.ready.borrow().contains(&p)
        }
        fn has_references(&self, p: Profile) -> Result<bool> {
            Ok(self.referenced.contains(&p))
        }
        fn pull(&self, p: Profile, _: Option<&Path>, _: &Progress) -> Result<()> {
            self.events.borrow_mut().push(format!("pull {}", p.id()));
            if self.fail_on == Some(p) {
                return Err(ForgeError::Host("interrupted download".into()));
            }
            self.ready.borrow_mut().push(p);
            Ok(())
        }
        fn verify(&self, p: Profile, _: &Progress) -> Result<()> {
            self.events.borrow_mut().push(format!("verify {}", p.id()));
            Ok(())
        }
        fn ensure_vms(&self, p: Profile, _: &Progress) -> Result<()> {
            self.events.borrow_mut().push(format!("ensure {}", p.id()));
            Ok(())
        }
    }
    #[test]
    fn all_prepares_three_profiles_and_explicitly_skips_sift() {
        let backend = Fake::default();
        let report = run(&backend, None, &progress::noop).unwrap();
        assert_eq!(
            report.prepared,
            [Profile::Tsurugi, Profile::Kali, Profile::Whonix]
        );
        assert_eq!(report.skipped.len(), 1);
        assert!(!backend.events.borrow().iter().any(|e| e == "pull sift"));
    }
    #[test]
    fn rerun_does_not_repull_ready_bases_but_still_verifies_them() {
        let backend = Fake::default();
        run(&backend, None, &progress::noop).unwrap();
        backend.events.borrow_mut().clear();
        run(&backend, None, &progress::noop).unwrap();
        assert!(
            !backend
                .events
                .borrow()
                .iter()
                .any(|e| e.starts_with("pull"))
        );
        assert!(backend.events.borrow().contains(&"verify whonix".into()));
    }
    #[test]
    fn referenced_missing_base_is_never_overwritten() {
        let backend = Fake {
            referenced: vec![Profile::Tsurugi],
            ..Fake::default()
        };
        assert!(run(&backend, None, &progress::noop).is_err());
        assert!(
            !backend
                .events
                .borrow()
                .iter()
                .any(|e| e.starts_with("pull"))
        );
    }
    #[test]
    fn failure_preserves_earlier_completed_downloads_for_resume() {
        let backend = Fake {
            fail_on: Some(Profile::Kali),
            ..Fake::default()
        };
        assert!(run(&backend, None, &progress::noop).is_err());
        assert_eq!(*backend.ready.borrow(), [Profile::Tsurugi]);
        assert!(!backend.events.borrow().contains(&"ensure kali".into()));
    }
    #[test]
    fn ready_sift_base_does_not_require_another_manual_download() {
        let backend = Fake {
            ready: RefCell::new(vec![Profile::Sift]),
            ..Fake::default()
        };
        let report = run(&backend, None, &progress::noop).unwrap();
        assert!(report.prepared.contains(&Profile::Sift));
        assert_eq!(report.skipped, Vec::<String>::new());
    }
}

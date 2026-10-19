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
    fn check_stopped(&self) -> Result<()>;
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
    run_selected(
        backend,
        &[
            Profile::Tsurugi,
            Profile::Kali,
            Profile::Whonix,
            Profile::Sift,
        ],
        sift_ova,
        progress,
    )
}

pub(crate) fn run_selected(
    backend: &impl Backend,
    profiles: &[Profile],
    sift_ova: Option<&Path>,
    progress: &Progress,
) -> Result<PrepareReport> {
    run_selected_stage(backend, profiles, sift_ova, progress, false)
}

pub(crate) fn download_selected(
    backend: &impl Backend,
    profiles: &[Profile],
    sift_ova: Option<&Path>,
    progress: &Progress,
) -> Result<PrepareReport> {
    run_selected_stage(backend, profiles, sift_ova, progress, true)
}

fn run_selected_stage(
    backend: &impl Backend,
    profiles: &[Profile],
    sift_ova: Option<&Path>,
    progress: &Progress,
    images_only: bool,
) -> Result<PrepareReport> {
    if profiles.is_empty()
        || profiles
            .iter()
            .enumerate()
            .any(|(i, p)| profiles[..i].contains(p))
    {
        return Err(ForgeError::InvalidInput(
            "select at least one profile, without duplicates".to_owned(),
        ));
    }
    if sift_ova.is_some() && !profiles.contains(&Profile::Sift) {
        return Err(ForgeError::InvalidInput(
            "--sift-ova requires the sift profile".to_owned(),
        ));
    }
    if sift_ova.is_some_and(|path| !path.is_file()) {
        return Err(ForgeError::InvalidInput(
            "--sift-ova must point to an existing local OVA file".to_owned(),
        ));
    }
    progress::message(
        progress,
        format!(
            "[1/{}] {}; guests must remain stopped",
            profiles.len() + 1,
            if images_only {
                "Checking image stage"
            } else {
                "Configuring Forge"
            }
        ),
    );
    if images_only {
        backend.check_stopped()?;
    } else {
        backend.setup()?;
    }
    let mut report = PrepareReport::default();
    for (index, profile) in profiles.iter().copied().enumerate() {
        progress::message(
            progress,
            format!(
                "[{}/{}] Preparing {}",
                index + 2,
                profiles.len() + 1,
                profile.id()
            ),
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
        if !images_only {
            backend.ensure_vms(profile, progress)?;
        }
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
        fn check_stopped(&self) -> Result<()> {
            self.events.borrow_mut().push("check stopped".into());
            Ok(())
        }
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
    fn images_are_verified_before_any_hook_or_vm_operation() {
        let backend = Fake::default();
        let selected = [Profile::Tsurugi, Profile::Kali, Profile::Whonix];
        let report = download_selected(&backend, &selected, None, &progress::noop).unwrap();
        assert_eq!(report.prepared, selected);
        let events = backend.events.borrow();
        assert_eq!(events.first().unwrap(), "check stopped");
        assert!(
            !events
                .iter()
                .any(|e| e == "setup" || e.starts_with("ensure"))
        );
        for p in selected {
            assert!(events.contains(&format!("verify {}", p.id())));
        }
    }

    #[test]
    fn download_failure_never_installs_hooks_or_creates_guests() {
        let backend = Fake {
            fail_on: Some(Profile::Kali),
            ..Fake::default()
        };
        assert!(
            download_selected(
                &backend,
                &[Profile::Tsurugi, Profile::Kali],
                None,
                &progress::noop
            )
            .is_err()
        );
        assert_eq!(*backend.ready.borrow(), [Profile::Tsurugi]);
        assert!(
            !backend
                .events
                .borrow()
                .iter()
                .any(|e| e == "setup" || e.starts_with("ensure"))
        );
    }

    #[test]
    fn selected_profiles_only_are_prepared_and_resume_without_repull() {
        let backend = Fake::default();
        let selected = [Profile::Kali, Profile::Whonix];
        let report = run_selected(&backend, &selected, None, &progress::noop).unwrap();
        assert_eq!(report.prepared, selected);
        assert_eq!(report.skipped, Vec::<String>::new());
        assert!(
            !backend
                .events
                .borrow()
                .iter()
                .any(|e| e.contains("tsurugi") || e.contains("sift"))
        );
        backend.events.borrow_mut().clear();
        run_selected(&backend, &selected, None, &progress::noop).unwrap();
        assert!(
            !backend
                .events
                .borrow()
                .iter()
                .any(|e| e.starts_with("pull"))
        );
        assert!(
            backend
                .events
                .borrow()
                .contains(&"verify whonix".to_owned())
        );
    }

    #[test]
    fn bad_selection_is_rejected_before_setup() {
        for selected in [vec![], vec![Profile::Kali, Profile::Kali]] {
            let backend = Fake::default();
            assert!(run_selected(&backend, &selected, None, &progress::noop).is_err());
            assert!(backend.events.borrow().is_empty());
        }
        let backend = Fake::default();
        assert!(
            run_selected(
                &backend,
                &[Profile::Kali],
                Some(Path::new("/missing.ova")),
                &progress::noop
            )
            .is_err()
        );
        assert!(backend.events.borrow().is_empty());
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

use std::process::Command;

fn forge() -> Command {
    Command::new(env!("CARGO_BIN_EXE_forge"))
}

#[test]
fn help_is_user_mode_only() {
    let out = forge().arg("--help").output().expect("help");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(text.contains("pull"));
    assert!(text.contains("create"));
    assert!(text.contains("clone"));
    assert!(text.contains("start"));
    assert!(text.contains("stop"));
    assert!(text.contains("connect"));
    assert!(text.contains("disconnect"));
    assert!(text.contains("status"));
    assert!(text.contains("list"));
    assert!(text.contains("delete"));
    assert!(
        !text.contains("doctor"),
        "doctor must not appear in user --help:\n{text}"
    );
    assert!(
        !text.lines().any(|line| {
            line.split_whitespace()
                .any(|word| word == "dev" || word == "`dev`")
        }),
        "dev command must be hidden from user --help:\n{text}"
    );
}

#[test]
fn doctor_is_not_a_user_command() {
    let out = forge().arg("doctor").output().expect("doctor");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(err.contains("forge dev doctor"), "{err}");
}

#[test]
fn v2_maze_is_rejected() {
    for cmd in ["image", "profile", "vm"] {
        let out = forge().arg(cmd).output().expect(cmd);
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success());
        assert!(err.contains("v2 command maze"), "{cmd}: {err}");
    }
}

#[test]
fn dev_help_exists() {
    let out = forge().args(["dev", "--help"]).output().expect("dev help");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(text.contains("doctor"));
    assert!(text.contains("xml"));
    assert!(text.contains("usb"));
    assert!(text.contains("cables"));
    assert!(text.contains("hook"));
}

#[test]
fn setup_exposes_only_the_two_explicit_modes() {
    let out = forge().args(["setup", "--help"]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(text.contains("standard") && text.contains("dedicated"));
    let out = forge()
        .args(["setup", "--network", "automatic"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("invalid value"));
}

#[test]
fn setup_refuses_running_guests_before_privileged_configuration() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("forge-setup-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let virsh = dir.join("virsh");
    fs::write(&virsh, "#!/bin/sh\ncase \"$3 $4\" in\n'uri ') echo qemu:///system;;\n'list --name') echo running-guest;;\n*) echo unexpected-virsh-call >&2; exit 99;;\nesac\n").unwrap();
    fs::set_permissions(&virsh, fs::Permissions::from_mode(0o755)).unwrap();
    let out = forge()
        .args(["setup", "--network", "standard"])
        .env("PATH", format!("{}:/usr/bin:/bin", dir.display()))
        .env("FORGE_DATA_DIR", dir.join("data"))
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("stop all QEMU guests"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn prepare_requires_all_and_exposes_music_and_manual_sift() {
    let out = forge().args(["prepare", "--help"]).output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    for flag in ["--all", "--no-music", "--music-dir", "--sift-ova"] {
        assert!(text.contains(flag), "{text}");
    }
    let out = forge().arg("prepare").output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--all"));
    let out = forge()
        .args(["prepare", "--all", "--sift-ova", "/no-such-forge.ova"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("existing local OVA"));
}

#[test]
fn prepare_error_stops_audio_and_keeps_user_library() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("forge-prepare-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    for (name, script) in [
        ("virsh", "#!/bin/sh\ncase \"$3 $4\" in\n'uri ') echo qemu:///system;;\n'list --name') sleep 0.1; echo running-guest;;\n*) exit 99;;\nesac\n".to_owned()),
        ("mpv", format!("#!/bin/sh\necho $$ > '{}'\nexec sleep 30\n", dir.join("player-pid").display())),
    ] {
        fs::write(dir.join(name), script).unwrap();
        fs::set_permissions(dir.join(name), fs::Permissions::from_mode(0o755)).unwrap();
    }
    fs::write(dir.join("track.mp3"), "fixture").unwrap();
    let out = forge()
        .args(["prepare", "--all", "--music-dir"])
        .arg(&dir)
        .env("PATH", format!("{}:/usr/bin:/bin", dir.display()))
        .env("FORGE_DATA_DIR", dir.join("data"))
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .output()
        .unwrap();
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(
        text.contains("Music: 1 tracks") && text.contains("stop all QEMU guests"),
        "{text}"
    );
    let pid = fs::read_to_string(dir.join("player-pid")).unwrap();
    assert!(!std::path::Path::new(&format!("/proc/{}", pid.trim())).exists());
    assert!(dir.join("track.mp3").exists());
    fs::remove_file(dir.join("player-pid")).unwrap();
    let silent = forge()
        .args(["prepare", "--all", "--no-music", "--music-dir"])
        .arg(&dir)
        .env("PATH", format!("{}:/usr/bin:/bin", dir.display()))
        .env("FORGE_DATA_DIR", dir.join("data"))
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .output()
        .unwrap();
    assert!(!silent.status.success());
    assert!(!dir.join("player-pid").exists());
    assert!(!String::from_utf8_lossy(&silent.stderr).contains("Music:"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn prepare_interrupt_stops_audio_and_removes_temporary_playlist() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    let dir = std::env::temp_dir().join(format!("forge-signal-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let virsh = format!(
        "#!/bin/sh\ncase \"$3 $4\" in\n'uri ') echo qemu:///system;;\n'list --name') echo $$ > '{}'; exec sleep 30;;\n*) exit 99;;\nesac\n",
        dir.join("worker-pid").display()
    );
    let player = format!(
        "#!/bin/sh\necho $$ > '{}'\nfor arg in \"$@\"; do case \"$arg\" in --playlist=*) echo \"${{arg#--playlist=}}\" > '{}';; esac; done\nexec sleep 30\n",
        dir.join("player-pid").display(),
        dir.join("playlist-path").display()
    );
    for (name, script) in [("virsh", virsh), ("mpv", player)] {
        fs::write(dir.join(name), script).unwrap();
        fs::set_permissions(dir.join(name), fs::Permissions::from_mode(0o755)).unwrap();
    }
    fs::write(dir.join("track.mp3"), "fixture").unwrap();
    let mut child = forge()
        .args(["prepare", "--all", "--music-dir"])
        .arg(&dir)
        .env("PATH", format!("{}:/usr/bin:/bin", dir.display()))
        .env("FORGE_DATA_DIR", dir.join("data"))
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while !(dir.join("playlist-path").exists() && dir.join("worker-pid").exists())
        && Instant::now() < until
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(dir.join("playlist-path").exists());
    let playlist = fs::read_to_string(dir.join("playlist-path")).unwrap();
    let player_pid = fs::read_to_string(dir.join("player-pid")).unwrap();
    assert!(
        Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(child.wait().unwrap().code(), Some(130));
    // This test sends SIGINT to Forge alone; a terminal normally signals its entire foreground group.
    let worker_pid = fs::read_to_string(dir.join("worker-pid")).unwrap();
    let _ = Command::new("kill")
        .args(["-TERM", worker_pid.trim()])
        .status();
    assert!(!std::path::Path::new(playlist.trim()).exists());
    assert!(!std::path::Path::new(&format!("/proc/{}", player_pid.trim())).exists());
    assert!(dir.join("track.mp3").exists());
    fs::remove_dir_all(dir).unwrap();
}

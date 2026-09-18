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
    assert!(text.contains("status"));
    assert!(text.contains("list"));
    assert!(text.contains("delete"));
    assert!(
        !text.contains("doctor"),
        "doctor must not appear in user --help:\n{text}"
    );
    assert!(
        !text.to_ascii_lowercase().contains("dev"),
        "dev must be hidden from user --help:\n{text}"
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
}

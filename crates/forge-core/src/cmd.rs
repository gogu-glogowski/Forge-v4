use std::cell::RefCell;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Output, Stdio};
use std::sync::Mutex;

use crate::error::{ForgeError, Result, command_fail};

pub fn command(name: &str) -> Command {
    let mut command = Command::new(name);
    command.env("LC_ALL", "C").env("LANG", "C");
    command
}

pub fn run(name: &str, args: &[&str]) -> Result<Output> {
    let output = command(name)
        .args(args)
        .output()
        .map_err(|error| ForgeError::Host(format!("cannot run {name}: {error}")))?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(command_fail(name, output.status, &output.stderr))
    }
}

pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).trim().to_owned()
}

pub fn stdout_or_stderr(output: &Output) -> String {
    let err = stderr(output);
    if err.is_empty() { stdout(output) } else { err }
}

pub fn exists(name: &str) -> bool {
    command("sh")
        .args(["-c", &format!("command -v {name}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[must_use]
pub fn first_existing(names: &[&str]) -> Option<String> {
    names
        .iter()
        .find(|name| exists(name))
        .map(|name| (*name).to_owned())
}

pub fn run_checked(name: impl AsRef<OsStr>, args: &[&str]) -> Result<String> {
    let name_str = name.as_ref().to_string_lossy().into_owned();
    let output = command(&name_str)
        .args(args)
        .output()
        .map_err(|error| ForgeError::Host(format!("cannot run {name_str}: {error}")))?;
    if output.status.success() {
        Ok(stdout(&output))
    } else {
        Err(command_fail(&name_str, output.status, &output.stderr))
    }
}

pub fn sudo(args: &[&str]) -> Result<String> {
    run_checked("sudo", args)
}

/// One root shell for the whole `pull`. Password at spawn; later jobs use this
/// process, not a new sudo (timestamp keepalive does not survive a TTY timeout).
pub struct SudoSession {
    child: Child,
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<BufReader<ChildStdout>>,
}

thread_local! {
    static PRIV: RefCell<Option<SudoSession>> = const { RefCell::new(None) };
}

const HELPER: &str = r#"
printf '%s\n' FORGE_PRIV_READY
while IFS= read -r hdr; do
  [ "$hdr" = "QUIT" ] && exit 0
  [ "$hdr" = "RUN" ] || { printf '%s\n' "FORGE_PRIV_ERR bad-header"; exit 1; }
  tmp=$(mktemp /tmp/forge-priv.XXXXXX) || { printf '%s\n' "FORGE_PRIV_ERR mktemp"; exit 1; }
  while IFS= read -r line; do
    [ "$line" = "FORGE_END" ] && break
    printf '%s\n' "$line" >> "$tmp"
  done
  if /bin/sh "$tmp"; then
    printf '%s\n' FORGE_PRIV_OK
  else
    printf '%s\n' "FORGE_PRIV_ERR $?"
  fi
  rm -f "$tmp"
done
"#;

impl SudoSession {
    pub fn start() -> Result<Self> {
        prompt_sudo()?;
        let mut child = command("sudo")
            .args(["-n", "/usr/bin/stdbuf", "-oL", "/bin/sh", "-c", HELPER])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| ForgeError::Host(format!("cannot spawn sudo helper: {error}")))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| ForgeError::Host("sudo helper has no stdin".to_owned()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ForgeError::Host("sudo helper has no stdout".to_owned()))?;
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|error| {
            ForgeError::Host(format!(
                "sudo helper died before READY ({error}). Run forge pull from a terminal."
            ))
        })?;
        if line.trim() != "FORGE_PRIV_READY" {
            let _ = child.kill();
            return Err(ForgeError::Host(format!(
                "sudo helper did not start (got {}). Type the password at the beginning, from a real terminal.",
                line.trim()
            )));
        }
        Ok(Self {
            child,
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(reader),
        })
    }

    pub fn run_script(&self, script: &str) -> Result<()> {
        validate_job_script(script)?;
        let mut stdin = self
            .stdin
            .lock()
            .map_err(|_| ForgeError::Host("sudo helper stdin poisoned".to_owned()))?;
        let mut stdout = self
            .stdout
            .lock()
            .map_err(|_| ForgeError::Host("sudo helper stdout poisoned".to_owned()))?;
        writeln!(stdin, "RUN").map_err(|e| ForgeError::Host(e.to_string()))?;
        for line in script.lines() {
            writeln!(stdin, "{line}").map_err(|e| ForgeError::Host(e.to_string()))?;
        }
        writeln!(stdin, "FORGE_END").map_err(|e| ForgeError::Host(e.to_string()))?;
        stdin.flush().map_err(|e| ForgeError::Host(e.to_string()))?;
        let mut line = String::new();
        stdout
            .read_line(&mut line)
            .map_err(|e| ForgeError::Host(e.to_string()))?;
        let line = line.trim();
        if line == "FORGE_PRIV_OK" {
            Ok(())
        } else {
            Err(ForgeError::Host(format!(
                "privileged helper failed: {line}"
            )))
        }
    }
}

impl Drop for SudoSession {
    fn drop(&mut self) {
        if let Ok(mut stdin) = self.stdin.lock() {
            let _ = writeln!(stdin, "QUIT");
            let _ = stdin.flush();
        }
        let _ = self.child.wait();
    }
}

fn prompt_sudo() -> Result<()> {
    let mut cmd = command("sudo");
    cmd.arg("-v");
    if let Ok(tty) = File::open("/dev/tty") {
        if let Ok(tty_in) = tty.try_clone() {
            cmd.stdin(Stdio::from(tty_in));
        }
        if let Ok(tty_out) = tty.try_clone() {
            cmd.stdout(Stdio::from(tty_out));
        }
        cmd.stderr(Stdio::from(tty));
    }
    let status = cmd
        .status()
        .map_err(|error| ForgeError::Host(format!("cannot run sudo -v: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(ForgeError::Host(
            "sudo password required at the start of pull (run from a terminal, not sudo forge)"
                .to_owned(),
        ))
    }
}

pub fn scope_priv<T>(session: SudoSession, body: impl FnOnce() -> Result<T>) -> Result<T> {
    struct Clear;
    impl Drop for Clear {
        fn drop(&mut self) {
            PRIV.with(|slot| {
                *slot.borrow_mut() = None;
            });
        }
    }
    PRIV.with(|slot| {
        *slot.borrow_mut() = Some(session);
    });
    let _clear = Clear;
    body()
}

/// During `scope_priv`, jobs go to the live root helper. Otherwise a fresh `sudo`.
pub fn priv_script(script: &str) -> Result<()> {
    validate_job_script(script)?;
    let used_helper = PRIV.with(|slot| slot.borrow().as_ref().map(|s| s.run_script(script)));
    match used_helper {
        Some(result) => result,
        None => sudo(&["sh", "-c", script]).map(|_| ()),
    }
}

pub(crate) fn validate_job_script(script: &str) -> Result<()> {
    for line in script.lines() {
        let t = line.trim();
        if t == "FORGE_END" || t == "QUIT" || t == "RUN" {
            return Err(ForgeError::Host(
                "internal: reserved token in privileged script".to_owned(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_script_rejects_protocol_tokens() {
        assert!(validate_job_script("chattr +i /x").is_ok());
        assert!(validate_job_script("echo FORGE_END").is_ok());
        assert!(validate_job_script("FORGE_END").is_err());
        assert!(validate_job_script("QUIT").is_err());
    }
}

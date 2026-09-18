use std::ffi::OsStr;
use std::process::{Command, Output, Stdio};

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

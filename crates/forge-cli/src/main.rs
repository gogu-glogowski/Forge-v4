use std::io::{self, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use forge_core::doctor;
use forge_core::progress::{ProgressEvent, format_bytes};
use forge_core::{APP_NAME, Forge, ForgeError, Profile, format_list, format_status};

#[derive(Parser)]
#[command(
    name = "forge",
    version,
    about = "Forge 4.0 — Fedora KVM lab, role-locked networking",
    long_about = None,
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Download and verify an upstream image into an immutable base
    Pull { profile: String },
    /// Create an overlay VM from a pulled base (role comes from the profile)
    Create {
        profile: String,
        name: Option<String>,
    },
    /// Clone a VM by name (not by file). Whonix: refused
    Clone { vm: String, new_name: String },
    /// Start a VM (refuses if base digest or role XML drifted)
    Start { vm: String },
    /// ACPI shutdown. --force cuts power
    Stop {
        vm: String,
        #[arg(long)]
        force: bool,
    },
    /// Running state plus role/network proof
    Status { vm: Option<String> },
    /// One inventory: profiles, base on disk, VM names
    List,
    /// Delete a Forge VM and its overlay. Base stays
    Delete { vm: String },
    /// Diagnostics (not shown in default --help)
    #[command(hide = true)]
    Dev {
        #[command(subcommand)]
        command: DevCommands,
    },
}

#[derive(Subcommand)]
enum DevCommands {
    /// Host fit: Fedora 44, KVM, libvirtd, Boxes rpm, no NAT on our domains
    Doctor,
    /// Domain XML vs role
    Xml { vm: String },
    /// create with --dry-run (prints XML, defines nothing)
    Create {
        profile: String,
        name: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    /// delete with --dry-run
    Delete {
        vm: String,
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(hint) = leftover_hint(&args) {
        eprintln!("{APP_NAME}: {hint}");
        return ExitCode::from(2);
    }
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{APP_NAME}: {error}");
            ExitCode::from(error.exit_code())
        }
    }
}

fn leftover_hint(args: &[String]) -> Option<String> {
    let first = args.first()?.as_str();
    if first == "--help" || first == "-h" || first == "--version" || first == "-V" {
        return None;
    }
    match first {
        "doctor" => Some("diagnostics live under `forge dev doctor`".to_owned()),
        "image" | "profile" | "vm" => {
            Some("v2 command maze is gone. See `forge --help`".to_owned())
        }
        _ => None,
    }
}

fn run(cli: Cli) -> Result<(), ForgeError> {
    match cli.command {
        Commands::Pull { profile } => {
            let profile: Profile = profile.parse()?;
            let forge = Forge::open()?;
            forge.pull(profile, &cli_progress())?;
            println!("pulled {}", profile.id());
            Ok(())
        }
        Commands::Create { profile, name } => {
            let profile: Profile = profile.parse()?;
            let forge = Forge::open()?;
            let created = forge.create(profile, name.as_deref(), false, &cli_progress())?;
            println!("created {}  uuid={}", created.name, created.uuid);
            println!("Open it in GNOME Boxes (qemu:///system). virt-manager is spare.");
            Ok(())
        }
        Commands::Clone { vm, new_name } => {
            let forge = Forge::open()?;
            let created = forge.clone_vm(&vm, &new_name, &cli_progress())?;
            println!("cloned {vm} -> {}", created.name);
            Ok(())
        }
        Commands::Start { vm } => {
            let forge = Forge::open()?;
            forge.start(&vm, &cli_progress())?;
            println!("started {vm}");
            println!("Daily display: GNOME Boxes. Spare: virt-manager.");
            Ok(())
        }
        Commands::Stop { vm, force } => {
            let forge = Forge::open()?;
            forge.stop(&vm, force)?;
            if force {
                println!("destroyed {vm}");
            } else {
                println!("shutdown {vm}");
            }
            Ok(())
        }
        Commands::Status { vm } => {
            let forge = Forge::open()?;
            let rows = forge.status(vm.as_deref())?;
            print!("{}", format_status(&rows));
            if rows.iter().any(|row| row.role_ok.is_err()) {
                return Err(ForgeError::Role("role/network proof failed".to_owned()));
            }
            Ok(())
        }
        Commands::List => {
            let forge = Forge::open()?;
            print!("{}", format_list(&forge.list()?));
            Ok(())
        }
        Commands::Delete { vm } => {
            let forge = Forge::open()?;
            let plan = forge.delete(&vm, false)?;
            println!("{plan}");
            Ok(())
        }
        Commands::Dev { command } => run_dev(command),
    }
}

fn run_dev(command: DevCommands) -> Result<(), ForgeError> {
    match command {
        DevCommands::Doctor => {
            let report = doctor::run()?;
            print!("{}", doctor::format_report(&report));
            if report.ok() {
                Ok(())
            } else {
                Err(ForgeError::Host("doctor found failures".to_owned()))
            }
        }
        DevCommands::Xml { vm } => {
            let forge = Forge::open()?;
            print!("{}", forge.dump_xml(&vm)?);
            Ok(())
        }
        DevCommands::Create {
            profile,
            name,
            dry_run,
        } => {
            if !dry_run {
                return Err(ForgeError::InvalidInput(
                    "forge dev create requires --dry-run (live create is `forge create`)"
                        .to_owned(),
                ));
            }
            let profile: Profile = profile.parse()?;
            let forge = Forge::open()?;
            let created = forge.create(profile, name.as_deref(), true, &cli_progress())?;
            print!("{}", created.xml);
            Ok(())
        }
        DevCommands::Delete { vm, dry_run } => {
            if !dry_run {
                return Err(ForgeError::InvalidInput(
                    "forge dev delete requires --dry-run (live delete is `forge delete`)"
                        .to_owned(),
                ));
            }
            let forge = Forge::open()?;
            println!("{}", forge.delete(&vm, true)?);
            Ok(())
        }
    }
}

fn cli_progress() -> impl Fn(&ProgressEvent) + Send + Sync {
    |event| match event {
        ProgressEvent::Message(text) => eprintln!("{text}"),
        ProgressEvent::Bytes { done, total, .. } => {
            if let Some(total) = total {
                eprint!(
                    "\r  {} / {} ({:.0}%)",
                    format_bytes(*done),
                    format_bytes(*total),
                    (*done as f64 / *total as f64) * 100.0
                );
            } else {
                eprint!("\r  {}", format_bytes(*done));
            }
            let _ = io::stderr().flush();
            if total.is_some_and(|total| *done >= total) {
                eprintln!();
            }
        }
    }
}

use std::io::{self, IsTerminal, Write};
use std::path::Path;
use forge_core::{ForgeError, NetworkMode};

fn answer(prompt: &str) -> Result<String, ForgeError> {
    print!("{prompt}"); io::stdout().flush()?;
    let mut input = String::new();
    if io::stdin().read_line(&mut input)? == 0 {
        return Err(ForgeError::InvalidInput("Installation cancelled: input closed".to_owned()));
    }
    Ok(input.trim().to_lowercase())
}

pub fn run(bundle: &Path) -> Result<(), ForgeError> {
    if !io::stdin().is_terminal() {
        return Err(ForgeError::InvalidInput("Run the interactive installer in a terminal".to_owned()));
    }
    let enabled = loop {
        match answer("Instalować z muzyką? [t/n]: ")?.as_str() {
            "t" | "tak" | "y" | "yes" => break true,
            "n" | "nie" | "no" => break false,
            _ => eprintln!("Wpisz t lub n."),
        }
    };
    println!("Kontrola hosta i instalacja prerequisites. Istniejące dane VM zostaną zachowane.");
    let music = crate::music::Session::default();
    let stop = music.stopper();
    ctrlc::set_handler(move || { stop(); std::process::exit(130); })
        .map_err(|e| ForgeError::Host(e.to_string()))?;
    let paths = forge_core::ForgePaths::discover();
    paths.ensure_user()?;
    let identity = hostnames()?;
    let player_present = std::process::Command::new("mpv").arg("--version").output().is_ok_and(|o| o.status.success());
    if enabled && player_present { music.start(&paths.meta, true, None); }
    let mut selected_mode = None;
    let result = forge_core::installer::run(bundle, || {
        if enabled && !player_present { music.start(&paths.meta, true, None); }
        println!("Sprawdziliśmy system i zainstalowaliśmy prerequisites. Przechodzimy do Forge.");
        let previous = std::fs::read_to_string("/var/lib/forge/network-mode").unwrap_or_else(|_| "standard".to_owned());
        let default: NetworkMode = previous.trim().parse()?;
        println!("1) STANDARD: internet hosta przez NAT, bez dodatkowego sprzętu.");
        println!("2) ADVANCED/Dedicated: host A, goście przez USB Ethernet B; osobne kable/interfejsy.");
        let mode = loop {
            match answer(&format!("Tryb [1/2; Enter: {default}]: "))?.as_str() {
                "" => break default,
                "1" | "standard" => break NetworkMode::Standard,
                "2" | "dedicated" | "advanced" => break NetworkMode::Dedicated,
                _ => eprintln!("Wybierz 1 albo 2."),
            }
        };
        if mode == NetworkMode::Dedicated {
            println!("Podłącz kabel A do Ethernet hosta oraz kabel B do adaptera USB Ethernet.");
            println!("A jest dla hosta, B dla gości. Mogą używać tego samego routera/dostawcy.");
            loop {
                let response = answer("Enter: sprawdź A/B; q: anuluj: ")?;
                if response == "q" { return Err(ForgeError::InvalidInput("Installation cancelled".to_owned())); }
                match forge_core::installer::check_dedicated_links() {
                    Ok((a,b)) => { println!("Wykryto A={a}, B={b}. Linki są aktywne; internet gości sprawdzimy oddzielnie."); break; }
                    Err(error) => eprintln!("{error}"),
                }
            }
        }
        let _ = std::process::Command::new("df").args(["-h", "/var/lib", "/home", &paths.meta.to_string_lossy()]).status();
        println!("Miejsce na dyski Forge — żadna opcja nie usuwa innych maszyn wirtualnych.");
        println!("1) Użyj obecnego miejsca; zachowaj istniejące maszyny Forge (zalecane przy aktualizacji).");
        println!("2) Przy nowej instalacji utwórz miejsce na /home; nie przenoś ani nie usuwaj istniejących VM.");
        let home = loop {
            match answer("Miejsce na dyski [1/2; Enter: zachowaj]: ")?.as_str() {
                "" | "1" => break false,
                "2" => break true,
                _ => eprintln!("Wybierz 1 albo 2."),
            }
        };
        selected_mode = Some(mode.to_string());
        Ok((mode, home))
    }, &crate::cli_progress());
    let report_path = paths.meta.join("installation-report.json");
    let after = hostnames()?;
    let identity_preserved = identity == after;
    let report = serde_json::json!({
        "target": "AlmaLinux 10 x86_64", "music": enabled, "network": selected_mode,
        "completed": result.is_ok() && identity_preserved, "error": result.as_ref().err().map(ToString::to_string),
        "prepared": result.as_ref().ok().map(|r| r.prepared.iter().map(|p| p.id()).collect::<Vec<_>>()),
        "skipped": result.as_ref().ok().map(|r| &r.skipped),
        "physical_dedicated_test": "pending", "post_reboot_test": "pending",
        "identity_before": identity, "identity_after": after, "identity_preserved": identity_preserved
    });
    std::fs::write(&report_path, serde_json::to_vec_pretty(&report).map_err(|e| ForgeError::Host(e.to_string()))?)?;
    result?;
    if !identity_preserved { return Err(ForgeError::Host("Host identity changed during installation; investigate before continuing".to_owned())); }
    println!("Instalacja i przygotowanie ukończone. Maszyny pozostają wyłączone.");
    println!("Raport: {}", report_path.display());
    println!("Uruchom ponownie komputer, aby nowe grupy/uprawnienia objęły sesję pulpitu. Następnie Boxes → QEMU System.");
    Ok(())
}

fn hostnames() -> Result<Vec<String>, ForgeError> {
    ["--static", "--transient", "--pretty"].iter().map(|kind| {
        let output = std::process::Command::new("hostnamectl").arg(kind).output()?;
        if !output.status.success() { return Err(ForgeError::Host("Cannot read hostname identity".to_owned())); }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }).collect()
}

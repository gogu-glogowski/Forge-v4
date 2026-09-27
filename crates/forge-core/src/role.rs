use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Isolated,
    WhonixWs,
    WhonixGw,
    OsintClearnet,
}

impl Role {
    /// Kali may hold the USB device itself. The Whonix gateway must not:
    /// its image names the first virtio NIC eth0 and expects 10.0.2.2 there.
    #[must_use]
    pub fn may_hold_dongle(self) -> bool {
        matches!(self, Self::OsintClearnet)
    }

    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Isolated => "isolated",
            Self::WhonixWs => "whonix-ws",
            Self::WhonixGw => "whonix-gw",
            Self::OsintClearnet => "osint-clearnet",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "isolated" => Some(Self::Isolated),
            "whonix-ws" => Some(Self::WhonixWs),
            "whonix-gw" => Some(Self::WhonixGw),
            "osint-clearnet" => Some(Self::OsintClearnet),
            _ => None,
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmPower {
    Running,
    Blocked,
    Paused,
    Shutdown,
    Shutoff,
    Crashed,
    Unknown,
}

impl VmPower {
    #[must_use]
    pub fn from_domstate(state: &str) -> Self {
        match state.trim() {
            "running" => Self::Running,
            "blocked" => Self::Blocked,
            "paused" => Self::Paused,
            "shutdown" => Self::Shutdown,
            "shut off" | "shutoff" => Self::Shutoff,
            "crashed" => Self::Crashed,
            _ => Self::Unknown,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Blocked => "blocked",
            Self::Paused => "paused",
            Self::Shutdown => "shutdown",
            Self::Shutoff => "shut off",
            Self::Crashed => "crashed",
            Self::Unknown => "unknown",
        }
    }

    #[must_use]
    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Running | Self::Blocked | Self::Paused | Self::Shutdown
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Role;

    #[test]
    fn only_kali_may_hold_dongle() {
        assert!(Role::OsintClearnet.may_hold_dongle());
        assert!(!Role::WhonixGw.may_hold_dongle());
        assert!(!Role::WhonixWs.may_hold_dongle());
        assert!(!Role::Isolated.may_hold_dongle());
    }
}

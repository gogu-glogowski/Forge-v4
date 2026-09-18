//! Forge 4.0 engine: fail-closed roles, signed bases, overlay VMs.

mod cmd;
pub mod doctor;
mod error;
mod hash;
mod lab;
mod ownership;
mod paths;
mod profile;
pub mod progress;
mod pull;
mod role;
mod usb;
mod verify;
mod virt;
mod xml;

pub use error::{ForgeError, Result};
pub use lab::{Created, Forge, InventoryRow, VmStatus, format_list, format_status};
pub use paths::ForgePaths;
pub use profile::{APP_NAME, Profile, SYSTEM_URI};
pub use progress::{ProgressEvent, format_bytes};
pub use role::{Role, VmPower};

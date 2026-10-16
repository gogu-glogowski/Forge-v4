//! Forge 4.2.0 engine: fail-closed roles, signed bases, overlay VMs.

mod boxes;
pub mod installer;
mod cmd;
pub mod doctor;
pub mod download;
mod error;
mod hash;
mod hook;
pub mod host;
mod hostnet;
mod lab;
mod network;
mod ownership;
mod paths;
mod prepare;
mod profile;
pub mod progress;
mod pull;
mod role;
mod security;
mod usb;
mod verify;
mod virt;
mod xml;

pub use boxes::{
    SourceStatus as BoxesSourceStatus, ensure_system_source as ensure_boxes_system_source,
};
pub use error::{ForgeError, Result};
pub use hook::root_entry;
pub use lab::{Created, DongleLink, Forge, InventoryRow, VmStatus, format_list, format_status};
pub use network::Mode as NetworkMode;
pub use paths::{ForgePaths, effective_uid};
pub use prepare::PrepareReport;
pub use profile::{APP_NAME, Profile, SYSTEM_URI};
pub use progress::{ProgressEvent, format_bytes};
pub use role::{Role, VmPower};

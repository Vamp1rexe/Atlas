//! Atlas core: everything the launcher does that is not UI.

pub mod auth;
pub mod cosmetics;
pub mod instances;
pub mod java;
pub mod launch;
pub mod modrinth;
pub mod net;
pub mod paths;
pub mod settings;
pub mod versions;

pub use net::{Progress, ProgressFn};

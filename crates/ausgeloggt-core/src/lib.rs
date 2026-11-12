//! ausgeloggt core: data model, encrypted vault, and merging of findings into accounts.
//!
//! Every discovery source (mail scan, password export, SSO list, data export ...)
//! produces [`model::Finding`]s. [`merge::merge_findings`] folds them into the
//! [`model::VaultData`], grouping by registrable domain. [`vault`] persists the
//! whole thing encrypted on disk.

pub mod domain;
pub mod insights;
pub mod merge;
pub mod model;
pub mod vault;

pub use model::*;

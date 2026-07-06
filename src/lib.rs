#![allow(rustdoc::bare_urls)]

//! Bindings for [`Microsoft.Management.Deployment`](https://learn.microsoft.com/windows/package-manager/)
//! (Windows Package Manager WinRT API).
//!
//! Generated from `Microsoft.Management.Deployment.winmd` via [`windows-bindgen`](https://crates.io/crates/windows-bindgen).
//! Doc comments are sourced from `PackageManager.idl`.
//!
//! # Usage
//!
//! ```rust,no_run
//! use winget_rs::PackageManager;
//!
//! let manager = PackageManager::new()?;
//! let catalogs = manager.GetPackageCatalogs()?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod bindings;

pub use bindings::Microsoft::Management::Deployment::*;

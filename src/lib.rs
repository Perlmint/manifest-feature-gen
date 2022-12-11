//! manifest-feature-gen helps generating features of cargo manifest
//!
//! ## Usage
//!
//! ```rust
//! enum Features {
//!     Feature1,
//!     Feature2,
//! }
//!
//! impl ToFeatureName for Features {
//!     fn to_feature_name(&self) -> String {
//!         // implement
//!     }
//! }
//!
//! let mut manifest = Manifest::new_with_env()?;
//! let optional_features = manifest.add_features([
//!     Features::Feature1,
//!     Features::Feature2,
//! ], |_, _| {}).unwrap();
//! manifest.write()?;
//! ```

/// Possible errors while using manifest-feature-gen
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Cannot find environment variable CARGO_MANIFEST_DIR")]
    EnvError,
    #[error("IO error - {0:?}")]
    IoError(#[from] std::io::Error),
    #[error("Failed to parse manifest - {0:?}")]
    ParseError(#[from] toml_edit::TomlError),
    #[error("Manifest is malformed - {0}")]
    MalformedManifest(String),
    #[error("Mutually exclusive features are enabled at the same time - {0:?}")]
    MutualExclusiveFeatureError(Vec<String>),
    // This is actually not an error. But, handling this as error can prevent useless build.
    #[error("Manifest is changed. Please re-run the build")]
    ManifestChanged,
}

/// Provide feature name for write to cargo manifest.
/// Recommend write in snake_case or kebab-case
pub trait ToFeatureName {
    fn to_feature_name(&self) -> String;
}

mod manifest;
pub use manifest::*;

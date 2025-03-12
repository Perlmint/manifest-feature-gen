use std::path::PathBuf;

use super::{DependencyHelper, Manifest};
use crate::{Error, ToFeatureName};

/// Cargo manifest representation for generating feature on build script.
///
/// Each feature adding methods will return chosen features.
pub struct ManifestWithFeatureCollector {
    manifest: Manifest,
    prevent_build_when_changed: bool,
}

impl ManifestWithFeatureCollector {
    /// Load cargo manifest of current crate
    pub fn new(prevent_build_when_changed: bool) -> Result<Self, Error> {
        let mut path: PathBuf = std::env::var("CARGO_MANIFEST_DIR")
            .map_err(|_| Error::EnvError)?
            .into();
        path.push("Cargo.toml");

        Ok(Self {
            manifest: Manifest::new(path)?,
            prevent_build_when_changed,
        })
    }

    /// Add features as an group
    ///
    /// This returns every chosen features.
    pub fn add_features<T: ToFeatureName>(
        &mut self,
        features: impl Iterator<Item = T>,
        dependency_setter: impl Fn(&T, &mut DependencyHelper),
    ) -> Result<Vec<T>, Error> {
        self.add_features_with_formatter(
            features,
            dependency_setter,
            ToFeatureName::to_feature_name,
        )
    }

    /// Add features with feature name formatter
    ///
    /// This returns every chosen features.
    pub fn add_features_with_formatter<T>(
        &mut self,
        features: impl Iterator<Item = T>,
        dependency_setter: impl Fn(&T, &mut DependencyHelper),
        feature_name_formatter: impl Fn(&T) -> String,
    ) -> Result<Vec<T>, Error> {
        let mut specified_features = Vec::new();

        self.manifest.add_features_with_formatter_and_handler(
            features,
            dependency_setter,
            feature_name_formatter,
            |feature_name, feature| {
                if std::env::var(format!(
                    "CARGO_FEATURE_{}",
                    feature_name.replace('-', "_").to_uppercase()
                ))
                .is_ok()
                {
                    specified_features.push(feature);
                }
            },
        )?;

        Ok(specified_features)
    }

    /// Add features to manifest. But, this features are mutually exclusive.\
    /// Enable multiple features at the same time, This method will fail
    pub fn add_mutually_exclusive_features<T: ToFeatureName>(
        &mut self,
        features: impl Iterator<Item = T>,
        dependency_setter: impl Fn(&T, &mut DependencyHelper),
    ) -> Result<Option<T>, Error> {
        self.add_mutually_exclusive_features_with_formatter(
            features,
            dependency_setter,
            ToFeatureName::to_feature_name,
        )
    }

    /// Add features to manifest with feature name formatter. But, this features are mutually exclusive.\
    /// Enable multiple features at the same time, This method will fail
    pub fn add_mutually_exclusive_features_with_formatter<T>(
        &mut self,
        features: impl Iterator<Item = T>,
        dependency_setter: impl Fn(&T, &mut DependencyHelper),
        feature_name_formatter: impl Fn(&T) -> String,
    ) -> Result<Option<T>, Error> {
        let specified =
            self.add_features_with_formatter(features, dependency_setter, &feature_name_formatter)?;
        if specified.len() > 1 {
            Err(Error::MutualExclusiveFeatureError(
                specified
                    .into_iter()
                    .map(|f| feature_name_formatter(&f))
                    .collect(),
            ))
        } else {
            Ok(specified.into_iter().next())
        }
    }

    /// Write Manifest file when changed.
    /// Returns `true` if manifest was changed.
    /// But, `prevent_build_when_changed` is set and manifest is changed, the method will fail.
    pub fn write(self) -> Result<bool, Error> {
        let changed = self.manifest.write()?;

        if self.prevent_build_when_changed && changed {
            Err(Error::ManifestChanged)
        } else {
            Ok(changed)
        }
    }
}

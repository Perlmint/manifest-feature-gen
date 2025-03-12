use std::{collections::HashSet, hash::Hash};

mod base;
pub use base::Manifest;
mod with_build_script;
pub use with_build_script::{BuildScriptExportDescriptor, ManifestWithBuildScript};
mod with_collector;
pub use with_collector::ManifestWithFeatureCollector;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Dependency {
    Simple(String),
    CrateFeature(String, String),
    OptionalCrateFeature(String, String),
}

impl Dependency {
    fn into_string(self) -> String {
        match self {
            Dependency::Simple(feature) => feature,
            Dependency::CrateFeature(crate_name, feature) => format!("{}/{}", crate_name, feature),
            Dependency::OptionalCrateFeature(crate_name, feature) => {
                format!("{}?/{}", crate_name, feature)
            }
        }
    }
}

/// This helper provides some safe way to specify dependency of generated feature
pub struct DependencyHelper<'a>(&'a str, HashSet<Dependency>);

/// Possible dependency error from `DependencyHelper`
#[derive(thiserror::Error, Debug, Clone, Copy)]
pub enum DependencyError {
    #[error("Already has conflicted dependency")]
    Conflict,
    #[error("Invalid dependency format")]
    InvalidDependencyFormat,
}

impl<'a> DependencyHelper<'a> {
    /// propagate feature to other crate
    pub fn propagate_to_crate(
        &mut self,
        crate_name: &str,
        optional: bool,
    ) -> Result<(), DependencyError> {
        self.add_crate_feature_dependency(crate_name, self.0, optional)
    }

    fn add_crate_feature_dependency(
        &mut self,
        crate_name: &str,
        feature_name: &str,
        optional: bool,
    ) -> Result<(), DependencyError> {
        let (crate_name, feature_name) = (crate_name.to_string(), feature_name.to_string());
        let conflict = if optional {
            Dependency::CrateFeature(crate_name, feature_name)
        } else {
            Dependency::OptionalCrateFeature(crate_name, feature_name)
        };
        if self.1.contains(&conflict) {
            Err(DependencyError::Conflict)
        } else {
            self.1.insert(match conflict {
                Dependency::OptionalCrateFeature(crate_name, feature_name) => {
                    Dependency::CrateFeature(crate_name, feature_name)
                }
                Dependency::CrateFeature(crate_name, feature_name) => {
                    Dependency::OptionalCrateFeature(crate_name, feature_name)
                }
                _ => unreachable!(),
            });
            Ok(())
        }
    }

    // add dependency for feature
    pub fn add_dependency(&mut self, dependency_name: &str) -> Result<(), DependencyError> {
        if dependency_name.contains('/') {
            let mut splitted_dependency_name = dependency_name.split('/');
            let crate_name = splitted_dependency_name
                .next()
                .ok_or(DependencyError::InvalidDependencyFormat)?;
            let feature_name = splitted_dependency_name
                .next()
                .ok_or(DependencyError::InvalidDependencyFormat)?;
            if splitted_dependency_name.next().is_some() {
                Err(DependencyError::InvalidDependencyFormat)
            } else {
                let (crate_name, optional) = if crate_name.ends_with('?') {
                    (&crate_name[0..(crate_name.len() - 2)], true)
                } else {
                    (crate_name, false)
                };
                self.add_crate_feature_dependency(crate_name, feature_name, optional)
            }
        } else {
            self.1
                .insert(Dependency::Simple(dependency_name.to_string()));
            Ok(())
        }
    }
}

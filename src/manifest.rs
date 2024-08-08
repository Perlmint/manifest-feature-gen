use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    path::PathBuf,
};

use fallible_iterator::FallibleIterator;
use toml_edit::{Array, Document, Formatted, Item, Table, Value};

use crate::{Error, ToFeatureName};

/// Cargo manifest representation for editing features.
///
/// This automatically remove generated features while loading.\
/// Generated features are identified by comment.\
/// For correct working, Do not remove auto-generated marking comment.
pub struct Manifest {
    path: PathBuf,
    original_features: HashMap<String, HashSet<String>>,
    document: toml_edit::Document,
    prevent_build_when_changed: bool,
}

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

const FEATURES_TABLE_NAME: &str = "features";
const AUTO_GENERATE_COMMENT: &str = concat!(" # auto-generated by ", env!("CARGO_CRATE_NAME"));

impl Manifest {
    /// Load cargo manifest from specified path
    pub fn new(path: PathBuf, prevent_build_when_changed: bool) -> Result<Self, Error> {
        let document = std::fs::read_to_string(&path)?;
        let mut document: toml_edit::Document = document.parse()?;

        let original_features = Self::collect_features(&document)?;

        let table = document.as_table_mut();
        if !table.contains_key(FEATURES_TABLE_NAME) {
            table.insert(FEATURES_TABLE_NAME, Item::Table(Table::new()));
        }

        let mut ret = Self {
            path,
            original_features,
            document,
            prevent_build_when_changed,
        };

        ret.clear_generated_features()?;

        Ok(ret)
    }

    /// Load cargo manifest of current crate
    pub fn new_with_env(prevent_build_when_changed: bool) -> Result<Self, Error> {
        let mut path: PathBuf = std::env::var("CARGO_MANIFEST_DIR")
            .map_err(|_| Error::EnvError)?
            .into();
        path.push("Cargo.toml");
        Self::new(path, prevent_build_when_changed)
    }

    fn collect_features(document: &Document) -> Result<HashMap<String, HashSet<String>>, Error> {
        if let Some(features) = document.as_table().get(FEATURES_TABLE_NAME) {
            let features = features
                .as_table()
                .ok_or_else(|| Error::MalformedManifest("features is not a table".to_string()))?;
            fallible_iterator::convert(features.into_iter().map(
                |(feature, deps)| -> Result<_, Error> {
                    let deps = deps.as_array().ok_or_else(|| {
                        Error::MalformedManifest(format!("feature({}) is not a array", feature))
                    })?;

                    Ok((
                        feature.to_string(),
                        fallible_iterator::convert(deps.into_iter().map(|dep| {
                            dep.as_str()
                                .ok_or_else(|| {
                                    Error::MalformedManifest(format!(
                                        "feature({}) has non string item as dependency",
                                        feature
                                    ))
                                })
                                .map(|dep| dep.to_string())
                        }))
                        .collect::<HashSet<_>>()?,
                    ))
                },
            ))
            .collect()
        } else {
            Ok(Default::default())
        }
    }

    fn clear_generated_features(&mut self) -> Result<(), Error> {
        if let Some(features) = self.document.as_table_mut().get_mut(FEATURES_TABLE_NAME) {
            let features = features
                .as_table_mut()
                .ok_or_else(|| Error::MalformedManifest("features is not a table".to_string()))?;
            let feature_names =
                fallible_iterator::convert(features.iter().filter_map(|(feature, item)| {
                    if let Some(deps) = item.as_array() {
                        (deps
                            .decor()
                            .suffix()
                            .and_then(|s| s.as_str())
                            .unwrap_or_default()
                            .trim()
                            == AUTO_GENERATE_COMMENT.trim())
                        .then(|| Ok(feature.to_string()))
                    } else {
                        Some(Err(Error::MalformedManifest(format!(
                            "value of feature({}) is not a array",
                            feature
                        ))))
                    }
                }))
                .collect::<Vec<_>>()?;
            for feature in feature_names {
                features.remove(&feature);
            }
        }

        Ok(())
    }

    /// Add feature to manifest.
    pub fn add_features<
        T: ToFeatureName,
        I: Iterator<Item = T>,
        F: Fn(&'_ T, &mut DependencyHelper<'_>),
    >(
        &mut self,
        feature_names: I,
        dependency_setter: F,
    ) -> Result<Vec<T>, Error> {
        let table = self.document.as_table_mut();
        let features = table.get_mut(FEATURES_TABLE_NAME).unwrap();
        let features = features.as_table_mut().unwrap();

        let mut specified_features = Vec::new();

        for feature in feature_names {
            let feature_name = feature.to_feature_name();
            let mut propagator = DependencyHelper(&feature_name, Default::default());
            let manual_dependent_feature = format!("__{}", feature_name);
            if features.contains_key(&manual_dependent_feature) {
                propagator
                    .1
                    .insert(Dependency::Simple(manual_dependent_feature));
            }
            dependency_setter(&feature, &mut propagator);
            let mut dependencies = propagator
                .1
                .into_iter()
                .map(Dependency::into_string)
                .collect::<Vec<_>>();
            dependencies.sort();
            let mut array = Array::from_iter(
                dependencies
                    .into_iter()
                    .map(|dep| Value::String(Formatted::<String>::new(dep))),
            );
            array.decor_mut().set_suffix(AUTO_GENERATE_COMMENT);
            features.insert(&feature_name, Item::Value(Value::Array(array)));

            if std::env::var(format!(
                "CARGO_FEATURE_{}",
                feature_name.replace('-', "_").to_uppercase()
            ))
            .is_ok()
            {
                specified_features.push(feature);
            }
        }

        Ok(specified_features)
    }

    /// Add features to manifest. But, this features are mutually exclusive.\
    /// Enable multiple features at the same time, This operation will fail
    pub fn add_mutually_exclusive_features<
        T: ToFeatureName,
        I: Iterator<Item = T>,
        F: Fn(&'_ T, &mut DependencyHelper<'_>),
    >(
        &mut self,
        feature_names: I,
        dependency_setter: F,
    ) -> Result<Option<T>, Error> {
        let specified = self.add_features(feature_names, dependency_setter)?;
        if specified.len() > 1 {
            Err(Error::MutualExclusiveFeatureError(
                specified.into_iter().map(|f| f.to_feature_name()).collect(),
            ))
        } else {
            Ok(specified.into_iter().next())
        }
    }

    fn check_is_changed(&self) -> Result<bool, Error> {
        let current_features = Self::collect_features(&self.document)?;

        Ok(current_features != self.original_features)
    }

    /// When manifest is changed, write back to the manifest file & return `Error::ManifestChanged`
    pub fn write(self) -> Result<bool, Error> {
        if self.check_is_changed()? {
            std::fs::write(&self.path, self.document.to_string())?;
            if self.prevent_build_when_changed {
                Err(Error::ManifestChanged)
            } else {
                Ok(true)
            }
        } else {
            Ok(false)
        }
    }
}

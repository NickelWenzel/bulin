//! The project file: a shader with its uniforms and clock.

use serde::{Deserialize, Serialize};

use crate::{Clock, Uniforms};

/// The format version this build writes and reads.
const VERSION: u32 = 1;

/// Everything a saved project restores.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Project {
    version: u32,
    pub shader: String,
    pub uniforms: Uniforms,
    /// The clock behind `time`, when it is on.
    pub clock: Option<Clock>,
}

/// Why a file couldn't be read as a project.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum Error {
    #[error("not a bulin project ({0})")]
    Invalid(String),
    #[error("bulin project version {0} is not supported, only version {VERSION}")]
    Unsupported(u32),
}

impl Project {
    pub fn new(shader: impl Into<String>, uniforms: Uniforms, clock: Option<Clock>) -> Self {
        Self {
            version: VERSION,
            shader: shader.into(),
            uniforms,
            clock,
        }
    }

    /// A fresh project: the default shader, no uniforms, no clock.
    pub fn untitled() -> Self {
        Self::new(crate::program::DEFAULT_SOURCE, Uniforms::default(), None)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("invariant: a project serializes to JSON")
    }

    pub fn from_json(json: &str) -> Result<Self, Error> {
        #[derive(Deserialize)]
        struct Header {
            version: Option<u32>,
        }

        let header: Header =
            serde_json::from_str(json).map_err(|error| Error::Invalid(error.to_string()))?;
        match header.version {
            None => Err(Error::Invalid("version missing".to_owned())),
            Some(VERSION) => {
                serde_json::from_str(json).map_err(|error| Error::Invalid(error.to_string()))
            }
            Some(version) => Err(Error::Unsupported(version)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::uniform::Type;

    #[test]
    fn a_project_survives_a_round_trip() {
        let mut uniforms = Uniforms::default();
        uniforms.add("radius", Type::Float(0.5)).unwrap();
        let mut clock = Clock::started();
        clock.advance(Duration::from_secs(2));
        let project = Project::new("// shader", uniforms, Some(clock));

        assert_eq!(Project::from_json(&project.to_json()), Ok(project));
    }

    #[test]
    fn files_without_a_version_are_not_projects() {
        assert_eq!(
            Project::from_json(r#"{"text_editor": {}}"#),
            Err(Error::Invalid("version missing".to_owned()))
        );
        assert_eq!(
            Project::from_json(r#"{"version": 2}"#),
            Err(Error::Unsupported(2))
        );
    }

    #[test]
    fn the_sample_projects_load() {
        for json in [
            include_str!("../../assets/test_project.bulin"),
            include_str!("../../assets/test_project_2.bulin"),
        ] {
            let project = Project::from_json(json).unwrap();
            assert!(project.shader.contains("customs.radius"));
            assert!(project.clock.is_some());
        }
    }
}

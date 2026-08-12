use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationOutcome {
    Valid(ValidatedFormat),
    Unchecked { extension: Option<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidatedFormat {
    Json,
    Xml,
}

impl ValidatedFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Json => "JSON valide",
            Self::Xml => "XML valide",
        }
    }
}

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Impossible de lire le bouchon '{}': {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("Le fichier bouchon est vide.")]
    Empty,

    #[error("Le JSON est invalide à la ligne {line}, colonne {column} : {message}")]
    InvalidJson {
        line: usize,
        column: usize,
        message: String,
    },

    #[error("Le XML n'est pas encodé en UTF-8 : {0}")]
    InvalidXmlEncoding(#[from] std::str::Utf8Error),

    #[error("Le XML est invalide à la position {position} : {message}")]
    InvalidXml { position: String, message: String },
}

pub fn validate_bouchon(path: &Path) -> Result<ValidationOutcome, ValidationError> {
    let contents = fs::read(path).map_err(|source| ValidationError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if contents.iter().all(u8::is_ascii_whitespace) {
        return Err(ValidationError::Empty);
    }

    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_lowercase());
    match extension.as_deref() {
        Some("json") => {
            if let Err(error) = serde_json::from_slice::<serde_json::Value>(&contents) {
                return Err(ValidationError::InvalidJson {
                    line: error.line(),
                    column: error.column(),
                    message: error.to_string(),
                });
            }
            Ok(ValidationOutcome::Valid(ValidatedFormat::Json))
        }
        Some("xml") => {
            let xml = std::str::from_utf8(&contents)?;
            if let Err(error) = roxmltree::Document::parse(xml) {
                return Err(ValidationError::InvalidXml {
                    position: error.pos().to_string(),
                    message: error.to_string(),
                });
            }
            Ok(ValidationOutcome::Valid(ValidatedFormat::Xml))
        }
        _ => Ok(ValidationOutcome::Unchecked { extension }),
    }
}

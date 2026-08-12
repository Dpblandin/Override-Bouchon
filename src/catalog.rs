use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::validation::{ValidationOutcome, validate_bouchon};

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("Impossible de lire la bibliothèque de bouchons '{}': {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BouchonValidation {
    Valid(ValidationOutcome),
    Invalid(String),
}

#[derive(Debug, Clone)]
pub struct BouchonEntry {
    path: PathBuf,
    name: String,
    validation: BouchonValidation,
    fingerprint: Option<ContentFingerprint>,
}

impl BouchonEntry {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn validation(&self) -> &BouchonValidation {
        &self.validation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ContentFingerprint {
    length: u64,
    hash: u64,
}

#[derive(Debug)]
pub struct BouchonCatalog {
    directory: PathBuf,
    entries: Vec<BouchonEntry>,
}

impl BouchonCatalog {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            entries: Vec::new(),
        }
    }

    pub fn load(directory: impl Into<PathBuf>) -> Result<Self, CatalogError> {
        let mut catalog = Self::new(directory);
        catalog.refresh()?;
        Ok(catalog)
    }

    pub fn refresh(&mut self) -> Result<(), CatalogError> {
        if !self.directory.is_dir() {
            self.entries.clear();
            return Ok(());
        }

        let directory_entries =
            fs::read_dir(&self.directory).map_err(|source| CatalogError::Io {
                path: self.directory.clone(),
                source,
            })?;
        let mut entries = Vec::new();

        for directory_entry in directory_entries {
            let directory_entry = directory_entry.map_err(|source| CatalogError::Io {
                path: self.directory.clone(),
                source,
            })?;
            let path = directory_entry.path();
            if !path.is_file() {
                continue;
            }

            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let validation = match validate_bouchon(&path) {
                Ok(outcome) => BouchonValidation::Valid(outcome),
                Err(error) => BouchonValidation::Invalid(error.to_string()),
            };
            let fingerprint = fs::read(&path).ok().map(|contents| fingerprint(&contents));
            entries.push(BouchonEntry {
                path,
                name,
                validation,
                fingerprint,
            });
        }

        entries.sort_by_cached_key(|entry| entry.name.to_lowercase());
        self.entries = entries;
        Ok(())
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn entries(&self) -> &[BouchonEntry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entry(&self, path: &Path) -> Option<&BouchonEntry> {
        self.entries.iter().find(|entry| entry.path == path)
    }

    pub fn matching_indices(&self, query: &str) -> Vec<usize> {
        let query = query.trim().to_lowercase();
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                (query.is_empty() || entry.name.to_lowercase().contains(&query)).then_some(index)
            })
            .collect()
    }

    pub fn identify_content(&self, path: &Path) -> Option<String> {
        let contents = fs::read(path).ok()?;
        let fingerprint = fingerprint(&contents);

        self.entries.iter().find_map(|entry| {
            if entry.fingerprint != Some(fingerprint) {
                return None;
            }
            let exact_match = fs::read(&entry.path)
                .ok()
                .is_some_and(|candidate| candidate == contents);
            exact_match.then(|| entry.name.clone())
        })
    }
}

fn fingerprint(contents: &[u8]) -> ContentFingerprint {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in contents {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    ContentFingerprint {
        length: contents.len() as u64,
        hash,
    }
}

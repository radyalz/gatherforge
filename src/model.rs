use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    #[default]
    Keep,
    Skip,
    Review,
}

impl Decision {
    pub fn label(self) -> &'static str {
        match self {
            Self::Keep => "Keep",
            Self::Skip => "Skip",
            Self::Review => "Review",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: u64,
    pub original: String,
    pub staged: String,
    pub filename: String,
    pub artist: String,
    pub raw_artist: String,
    pub title: String,
    pub format: String,
    pub size: u64,
    pub action: Decision,
    pub duplicate_of: Option<u64>,
    pub duplicate_kind: Option<String>,
    pub published: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    #[default]
    Review,
    Staging,
    Staged,
    Published,
    Finalizing,
    Finalized,
}
impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Review => "Ready for review",
            Self::Staging => "Staging",
            Self::Staged => "Staged",
            Self::Published => "Published",
            Self::Finalizing => "Finalizing",
            Self::Finalized => "Finalized",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub version: u32,
    pub phase: Phase,
    pub tracks: Vec<Track>,
    pub aliases: BTreeMap<String, String>,
}
impl Default for Library {
    fn default() -> Self {
        Self {
            version: 3,
            phase: Phase::Review,
            tracks: vec![],
            aliases: BTreeMap::new(),
        }
    }
}
impl Library {
    pub fn kept(&self) -> usize {
        self.tracks
            .iter()
            .filter(|t| t.action == Decision::Keep)
            .count()
    }
    pub fn reviews(&self) -> usize {
        self.tracks
            .iter()
            .filter(|t| t.action == Decision::Review)
            .count()
    }
    pub fn duplicates(&self) -> usize {
        self.tracks
            .iter()
            .filter(|t| t.duplicate_of.is_some())
            .count()
    }
    pub fn folders(&self) -> BTreeMap<String, usize> {
        let mut map = BTreeMap::new();
        for t in &self.tracks {
            if t.action != Decision::Skip {
                *map.entry(t.artist.clone()).or_insert(0) += 1;
            }
        }
        map
    }
}

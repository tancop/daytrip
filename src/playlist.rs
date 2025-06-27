use librespot::core::{Error as SpotifyError, SpotifyId};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum SavedTrack {
    Id(String),
    Object { id: String, name: Option<String> },
}

impl SavedTrack {
    pub fn id(&self) -> Result<SpotifyId, SpotifyError> {
        match self {
            SavedTrack::Id(id) => SpotifyId::from_uri(id),
            SavedTrack::Object { id, .. } => SpotifyId::from_uri(id),
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            SavedTrack::Id(_) => None,
            SavedTrack::Object { name, .. } => name.as_ref().map(|s| s.as_str()),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) struct SavedPlaylist {
    pub title: String,
    pub tracks: Vec<SavedTrack>,
}

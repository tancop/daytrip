use librespot::core::{Error as SpotifyError, SpotifyId};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum Track {
    Id(String),
    Object { id: String, name: Option<String> },
}

impl Track {
    pub fn id(&self) -> Result<SpotifyId, SpotifyError> {
        match self {
            Track::Id(id) => SpotifyId::from_uri(id),
            Track::Object { id, .. } => SpotifyId::from_uri(id),
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            Track::Id(_) => None,
            Track::Object { name, .. } => name.as_ref().map(|s| s.as_str()),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Playlist {
    pub title: String,
    pub tracks: Vec<Track>,
}

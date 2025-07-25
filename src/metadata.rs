use std::path::Path;

use itertools::Itertools;
use once_cell::sync::OnceCell;

use librespot::{
    metadata::audio::{AudioFileFormat, AudioItem, UniqueFields},
    playback::config::{Bitrate, PlayerConfig},
};
use regex::Regex;

use crate::OutputFormat;

pub(crate) static REGEX_FILTER: OnceCell<Regex> = OnceCell::new();

pub fn get_input_format(config: &PlayerConfig, audio_item: &AudioItem) -> Option<AudioFileFormat> {
    let formats = match config.bitrate {
        Bitrate::Bitrate96 => [
            AudioFileFormat::OGG_VORBIS_96,
            AudioFileFormat::MP3_96,
            AudioFileFormat::OGG_VORBIS_160,
            AudioFileFormat::MP3_160,
            AudioFileFormat::MP3_256,
            AudioFileFormat::OGG_VORBIS_320,
            AudioFileFormat::MP3_320,
        ],
        Bitrate::Bitrate160 => [
            AudioFileFormat::OGG_VORBIS_160,
            AudioFileFormat::MP3_160,
            AudioFileFormat::OGG_VORBIS_96,
            AudioFileFormat::MP3_96,
            AudioFileFormat::MP3_256,
            AudioFileFormat::OGG_VORBIS_320,
            AudioFileFormat::MP3_320,
        ],
        Bitrate::Bitrate320 => [
            AudioFileFormat::OGG_VORBIS_320,
            AudioFileFormat::MP3_320,
            AudioFileFormat::MP3_256,
            AudioFileFormat::OGG_VORBIS_160,
            AudioFileFormat::MP3_160,
            AudioFileFormat::OGG_VORBIS_96,
            AudioFileFormat::MP3_96,
        ],
    };

    match formats
        .iter()
        .find_map(|format| match audio_item.files.get(format) {
            Some(&file_id) => Some((*format, file_id)),
            _ => None,
        }) {
        Some(t) => Some(t.0),
        None => {
            log::warn!(
                "<{}> is not available in any supported format",
                audio_item.name
            );
            None
        }
    }
}

/// Replace characters illegal in a path on Windows or Linux
pub(crate) fn legalize_name(name: &str) -> String {
    name.replace(&['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
}

pub async fn get_file_name(
    audio_item: &AudioItem,
    template: &str,
    track_number: Option<u32>,
    extension: Option<&str>,
) -> String {
    let name = match &audio_item.unique_fields {
        UniqueFields::Track { artists, .. } => {
            // music
            let title = template
                .replace(
                    "%a",
                    &artists
                        .first()
                        .map(|artist| artist.name.to_owned())
                        .unwrap_or("".to_owned()),
                )
                .replace("%A", &artists.iter().map(|artist| &*artist.name).join(", "))
                .replace("%t", &legalize_name(&audio_item.name))
                .replace("%n", &format!("{:02}", track_number.unwrap_or(0)));

            match extension {
                Some(ext) => format!("{}.{}", title, ext),
                None => title,
            }
        }
        UniqueFields::Episode { show_name, .. } => {
            // podcast

            let title = template
                .replace("%a", &show_name)
                .replace("%t", &legalize_name(&audio_item.name))
                .replace("%n", &format!("{:02}", track_number.unwrap_or(0)));

            match extension {
                Some(ext) => format!("{}.{}", title, ext),
                None => title,
            }
        }
    };

    if let Some(regex) = REGEX_FILTER.get() {
        regex.replace_all(&name, "").into_owned()
    } else {
        name
    }
}

pub fn try_get_format_from_file_name(name: &str) -> Option<OutputFormat> {
    let extension = name.split('.').last()?;
    OutputFormat::from_extension(extension)
}

pub fn try_get_format_from_path(path: Option<&Path>) -> Option<OutputFormat> {
    match path {
        Some(path) => match path.extension().and_then(|ext| ext.to_str()) {
            Some(ext) => OutputFormat::from_extension(ext),
            None => None,
        },
        None => None,
    }
}

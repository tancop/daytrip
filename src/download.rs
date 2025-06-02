use anyhow::{anyhow, bail};
use std::{
    borrow::Cow,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::Context;
use librespot::{
    core::{Session, SpotifyId, spotify_id::SpotifyItemType},
    metadata::{
        Album, Metadata, Playlist, Show,
        artist::ArtistsWithRole,
        audio::{AudioFileFormat, AudioItem, UniqueFields},
    },
    playback::{
        audio_backend,
        config::{AudioFormat, Bitrate, PlayerConfig},
        mixer::NoOpVolume,
        player::{Player, PlayerEvent},
    },
};
use once_cell::sync::{Lazy, OnceCell};
use regex::Regex;
use tokio::{
    fs::{File, create_dir_all},
    process::Child,
};

use crate::{Args, OutputFormat};

static FEATURE_TAG_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r" ?\((?:feat\.?|ft\.?|with) .+\)").unwrap());

static REGEX_FILTER: OnceCell<Regex> = OnceCell::new();

/// Replace characters illegal in a path on Windows or Linux
fn legalize_name(name: String) -> String {
    name.replace(&['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
}

fn get_format_extension(format: &OutputFormat) -> &str {
    match format {
        OutputFormat::Opus => "opus",
        OutputFormat::Mp3 => "mp3",
        OutputFormat::Ogg => "ogg",
        OutputFormat::Wav => "wav",
    }
}

fn get_input_format(config: &PlayerConfig, audio_item: &AudioItem) -> Option<AudioFileFormat> {
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

fn get_bitrate(format: &AudioFileFormat) -> u32 {
    match format {
        AudioFileFormat::OGG_VORBIS_96 => 96,
        AudioFileFormat::OGG_VORBIS_160 => 160,
        AudioFileFormat::OGG_VORBIS_320 => 320,
        AudioFileFormat::MP3_256 => 256,
        AudioFileFormat::MP3_320 => 320,
        AudioFileFormat::MP3_160 => 160,
        AudioFileFormat::MP3_96 => 96,
        AudioFileFormat::MP3_160_ENC => 160,
        AudioFileFormat::AAC_24 => 24,
        AudioFileFormat::AAC_48 => 48,
        AudioFileFormat::FLAC_FLAC => 1411,
        AudioFileFormat::XHE_AAC_24 => 24,
        AudioFileFormat::XHE_AAC_16 => 16,
        AudioFileFormat::XHE_AAC_12 => 12,
        AudioFileFormat::FLAC_FLAC_24BIT => 1411,
        AudioFileFormat::AAC_160 => 160,
        AudioFileFormat::AAC_320 => 320,
        AudioFileFormat::MP4_128 => 128,
        AudioFileFormat::OTHER5 => 0,
    }
}

fn remove_feature_tag(mut title: String) -> String {
    if let Some(matched) = FEATURE_TAG_REGEX.find(&title) {
        title.replace_range(matched.range(), "")
    }
    title
}

/// Format a track number to start with the right amount of zeros
fn format_track_number(number: usize, total_tracks: usize) -> String {
    let digits = (total_tracks as f32).log10().ceil() as usize;

    format!("{:0width$}", number, width = digits)
}

fn get_artists_line(artists: &ArtistsWithRole) -> String {
    artists
        .iter()
        .map(|artist| &*artist.name)
        .collect::<Vec<&str>>()
        .join(", ")
}

fn get_ffmpeg_command(
    args: &Args,
    input_format: Option<AudioFileFormat>,
    output_file_name: &Path,
) -> Result<Child, std::io::Error> {
    // Read track as stereo signed 16-bit PCM and encode into audio file
    const COMMON_ARGS: &[&str] = &[
        "-y",
        "-hide_banner",
        "-loglevel",
        "error",
        "-f",
        "s16le",
        "-ac",
        "2",
        "-i",
        "temp.pcm",
    ];
    if args.format == OutputFormat::Wav || input_format.is_none() {
        tokio::process::Command::new("ffmpeg")
            .args(COMMON_ARGS)
            .arg(output_file_name)
            .spawn()
    } else {
        // Set output bitrate to match downloaded audio
        let bitrate = get_bitrate(&input_format.unwrap());
        tokio::process::Command::new("ffmpeg")
            .args(COMMON_ARGS)
            .arg("-b:a")
            // Convert bitrate to bps
            .arg((bitrate * 1000).to_string())
            .arg(output_file_name)
            .spawn()
    }
}

pub struct Loader {
    session: Session,
}

impl Loader {
    pub fn new(session: Session) -> Self {
        Self { session }
    }

    pub async fn download_track(
        &self,
        track_ref: SpotifyId,
        args: &Args,
        path_prefix: Option<&Path>,
        name_prefix: Option<&str>,
    ) -> anyhow::Result<()> {
        let output_file_name: String;

        let extension = get_format_extension(&args.format);

        let config = PlayerConfig::default();

        let mut input_format: Option<AudioFileFormat> = None;

        if let Ok(audio_item) = AudioItem::get_file(&self.session, track_ref).await {
            input_format = get_input_format(&config, &audio_item);
            match audio_item.unique_fields {
                UniqueFields::Track { artists, .. } => {
                    // music
                    let artists_line = get_artists_line(&artists);

                    let title = if args.remove_feature_tags {
                        remove_feature_tag(audio_item.name)
                    } else {
                        audio_item.name
                    };

                    let title = match REGEX_FILTER.get() {
                        Some(re) => re.replace_all(&title, ""),
                        None => Cow::from(&title),
                    };

                    output_file_name = format!(
                        "{}{} - {}.{}",
                        name_prefix.unwrap_or(""),
                        if args.main_artist_only {
                            artists
                                .first()
                                .map(|artist| artist.name.to_owned())
                                .unwrap_or("".to_owned())
                        } else {
                            artists_line
                        },
                        title,
                        extension
                    );
                }
                UniqueFields::Episode { show_name, .. } => {
                    // podcast

                    let title = if args.remove_feature_tags {
                        remove_feature_tag(audio_item.name)
                    } else {
                        audio_item.name
                    };

                    let title = match REGEX_FILTER.get() {
                        Some(re) => re.replace_all(&title, ""),
                        None => Cow::from(&title),
                    };

                    output_file_name = format!(
                        "{}{} - {}.{}",
                        name_prefix.unwrap_or(""),
                        show_name,
                        title,
                        extension
                    );
                }
            };
        } else {
            log::warn!("Failed to get audio item name, falling back to ID");
            output_file_name = format!("{}.{}", track_ref.to_base62().unwrap(), extension);
        }

        let search_path = match path_prefix {
            Some(prefix) => &prefix.join(&output_file_name),
            None => Path::new(&output_file_name),
        };

        if !args.force_download && search_path.exists() {
            println!("Skipping {output_file_name}");
            return Ok(());
        }
        println!("Downloading {output_file_name}");

        let output_file_name = match path_prefix {
            Some(prefix) => prefix.join(&legalize_name(output_file_name)),
            None => legalize_name(output_file_name).into(),
        };

        let backend = audio_backend::find(Some("pipe".to_owned()))
            .ok_or_else(|| anyhow!("Failed to find audio backend"))?;

        let player = Player::new(
            config,
            self.session.clone(),
            Box::new(NoOpVolume),
            move || backend(Some("temp.pcm".into()), AudioFormat::S16),
        );

        let mut rx = player.get_player_event_channel();

        player.load(track_ref, true, 0);

        let player_ref = player.clone();

        let success = Arc::from(AtomicBool::from(true));
        let success2 = success.clone();

        let task = tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                if let PlayerEvent::Unavailable { .. } = event {
                    success2.store(false, Ordering::Relaxed);
                    player_ref.stop();
                    break;
                }
            }
        });

        player.await_end_of_track().await;
        task.abort();

        if !success.load(Ordering::Relaxed) {
            bail!("Failed to download track");
        }

        let mut cmd = get_ffmpeg_command(args, input_format, &output_file_name)?;

        cmd.wait().await.context("Failed to wait for ffmpeg")?;

        let file = File::create("temp.pcm")
            .await
            .context("Failed to open temp.pcm for cleanup")?;

        file.set_len(0)
            .await
            .context("Failed to truncate temp.pcm")?;

        Ok(())
    }

    pub async fn download_playlist(
        &self,
        playlist_ref: SpotifyId,
        args: Args,
    ) -> anyhow::Result<()> {
        let plist = Playlist::get(&self.session, &playlist_ref).await.unwrap();
        println!("Downloading playlist {}", plist.name());

        let name = plist.name();
        let folder = Path::new(&name);

        create_dir_all(folder)
            .await
            .context("Failed to create playlist folder")?;

        if args.number_tracks {
            let length = plist.length;
            let mut idx = 1;

            for track_id in plist.tracks() {
                self.download_track_with_retry(
                    track_id.clone(),
                    &args,
                    Some(folder),
                    Some(&format_track_number(idx, length as usize)),
                )
                .await?;
                idx += 1;
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        } else {
            for track_id in plist.tracks() {
                self.download_track_with_retry(track_id.clone(), &args, Some(folder), None)
                    .await?;
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }

        Ok(())
    }

    pub async fn download_album(&self, playlist_ref: SpotifyId, args: Args) -> anyhow::Result<()> {
        let album = Album::get(&self.session, &playlist_ref).await.unwrap();
        let artists = if args.main_artist_only {
            album
                .artists
                .first()
                .map(|artist| artist.name.to_owned())
                .unwrap_or("".to_owned())
        } else {
            album
                .artists
                .iter()
                .map(|artist| &*artist.name)
                .collect::<Vec<&str>>()
                .join(", ")
        };

        let folder_name = format!("{} - {}", artists, album.name);
        let folder = Path::new(&folder_name);

        create_dir_all(folder)
            .await
            .context("Failed to create album folder")?;

        println!("Downloading album {} by {}", album.name, artists);

        if args.number_tracks {
            let length = album.tracks().count();
            let mut idx = 1;

            for track_id in album.tracks() {
                self.download_track_with_retry(
                    track_id.clone(),
                    &args,
                    Some(folder),
                    Some(&format_track_number(idx, length as usize)),
                )
                .await?;
                idx += 1;
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        } else {
            for track_id in album.tracks() {
                self.download_track_with_retry(track_id.clone(), &args, Some(folder), None)
                    .await?;
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }

        Ok(())
    }

    pub async fn download_show(&self, playlist_ref: SpotifyId, args: Args) -> anyhow::Result<()> {
        let show = Show::get(&self.session, &playlist_ref).await.unwrap();
        println!("Downloading show {} by {}", show.name, show.publisher);

        let folder = Path::new(&show.name);

        create_dir_all(folder)
            .await
            .context("Failed to create show folder")?;

        if args.number_tracks {
            let length = show.episodes.len();
            let mut idx = 1;

            for episode_id in show.episodes.iter() {
                self.download_track_with_retry(
                    episode_id.clone(),
                    &args,
                    Some(folder),
                    Some(&format_track_number(idx, length as usize)),
                )
                .await?;
                idx += 1;
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        } else {
            for episode_id in show.episodes.iter() {
                self.download_track_with_retry(episode_id.clone(), &args, Some(folder), None)
                    .await?;
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }

        Ok(())
    }

    async fn download_track_with_retry(
        &self,
        track_ref: SpotifyId,
        args: &Args,
        path_prefix: Option<&Path>,
        name_prefix: Option<&str>,
    ) -> anyhow::Result<()> {
        let mut tries = 1;
        while let Err(e) = self
            .download_track(track_ref, &args, path_prefix, name_prefix)
            .await
        {
            tries += 1;
            if tries > args.max_tries {
                log::error!("Reached max retries, aborting");
                return Err(e);
            } else {
                log::warn!("Failed to download {}, retrying: {}", track_ref, e);
            }
        }

        Ok(())
    }

    pub async fn download(&self, item_ref: SpotifyId, args: Args) {
        if let Some(filter) = &args.track_title_filter {
            match Regex::new(filter) {
                Ok(re) => {
                    _ = REGEX_FILTER.try_insert(re).unwrap();
                }
                Err(e) => {
                    log::warn!("Invalid regex filter: {}", e);
                }
            };
        }

        if let Err(e) = match item_ref.item_type {
            SpotifyItemType::Track => {
                self.download_track_with_retry(item_ref, &args, None, None)
                    .await
            }
            SpotifyItemType::Album => self.download_album(item_ref, args).await,
            SpotifyItemType::Playlist => self.download_playlist(item_ref, args).await,
            SpotifyItemType::Episode => {
                self.download_track_with_retry(item_ref, &args, None, None)
                    .await
            }
            SpotifyItemType::Show => self.download_show(item_ref, args).await,
            _ => {
                log::error!("Unsupported item type: {:?}", item_ref.item_type);
                std::process::exit(1);
            }
        } {
            log::error!("Failed to download: {}", e);
        }
    }
}

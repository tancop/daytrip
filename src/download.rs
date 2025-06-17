use crate::metadata::{
    REGEX_FILTER, get_input_format, try_get_format_from_file_name, try_get_format_from_path,
};
use anyhow::{anyhow, bail};
use itertools::Itertools;
use std::{
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
        audio::{AudioFileFormat, AudioItem, UniqueFields},
    },
    playback::{
        audio_backend,
        config::{AudioFormat, PlayerConfig},
        mixer::NoOpVolume,
        player::{Player, PlayerEvent},
    },
};
use regex::Regex;
use tokio::{
    fs::{File, create_dir_all},
    process::{Child, Command},
};

use crate::{Args, OutputFormat, metadata::get_file_name};

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

pub(crate) trait CommandExt {
    fn with_metadata(&mut self, name: &str, value: &str) -> &mut Self;
}

impl CommandExt for Command {
    fn with_metadata(&mut self, name: &str, value: &str) -> &mut Self {
        self.arg("-metadata").arg(format!("{}={}", name, value))
    }
}

fn get_ffmpeg_command(
    input_format: Option<AudioFileFormat>,
    output_format: OutputFormat,
    output_file_name: &Path,
    audio_item: &AudioItem,
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

    let mut cmd = Command::new("ffmpeg");
    let cmd = cmd
        .args(COMMON_ARGS)
        .with_metadata("title", &audio_item.name)
        .with_metadata("comment", &audio_item.uri);

    let cmd = match &audio_item.unique_fields {
        UniqueFields::Episode {
            show_name,
            description,
            ..
        } => cmd
            .with_metadata("show", &show_name)
            .with_metadata("description", &description),
        UniqueFields::Track {
            artists,
            album,
            album_artists,
            number,
            ..
        } => cmd
            .with_metadata(
                "artist",
                &artists.iter().map(|artist| &*artist.name).join(", "),
            )
            .with_metadata("album", &album)
            .with_metadata("album_artist", &album_artists.iter().join(", "))
            .with_metadata("track", &number.to_string()),
    };

    if output_format == OutputFormat::Wav || input_format.is_none() {
        cmd.arg(output_file_name).spawn()
    } else {
        // Set output bitrate to match downloaded audio
        let bitrate = get_bitrate(&input_format.unwrap());
        cmd.arg("-b:a")
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
        audio_item: &AudioItem,
        output_path: &Path,
        output_format: OutputFormat,
        force_download: bool,
    ) -> anyhow::Result<()> {
        let config = PlayerConfig::default();

        let input_format = get_input_format(&config, audio_item);

        if !force_download && output_path.exists() {
            println!("Skipping {}", output_path.to_string_lossy());
            return Ok(());
        }
        if let Some(parent) = output_path.parent() {
            create_dir_all(parent).await?;
        }
        println!("Downloading {}", output_path.to_string_lossy());

        let backend = audio_backend::find(Some("pipe".to_owned()))
            .ok_or_else(|| anyhow!("Failed to find audio backend"))?;

        let player = Player::new(
            config,
            self.session.clone(),
            Box::new(NoOpVolume),
            move || backend(Some("temp.pcm".into()), AudioFormat::S16),
        );

        let mut rx = player.get_player_event_channel();

        player.load(audio_item.track_id.clone(), true, 0);

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

        let mut cmd = get_ffmpeg_command(input_format, output_format, &output_path, &audio_item)?;

        cmd.wait().await.context("Failed to wait for ffmpeg")?;

        let file = File::create("temp.pcm")
            .await
            .context("Failed to open temp.pcm for cleanup")?;

        file.set_len(0)
            .await
            .context("Failed to truncate temp.pcm")?;

        Ok(())
    }

    async fn download_track_with_retry(
        &self,
        audio_item: &AudioItem,
        output_path: &Path,
        output_format: OutputFormat,
        force_download: bool,
        max_tries: u32,
    ) -> anyhow::Result<()> {
        let mut tries = 1;
        while let Err(e) = self
            .download_track(audio_item, output_path, output_format, force_download)
            .await
        {
            tries += 1;
            if tries > max_tries {
                log::error!("Reached max retries, aborting");
                return Err(e);
            } else {
                log::warn!(
                    "Failed to download {}, retrying: {}",
                    audio_item.track_id,
                    e
                );
            }
        }

        Ok(())
    }

    pub async fn download_tracks(
        &self,
        tracks: impl Iterator<Item = &SpotifyId>,
        folder: &Path,
        output_format: Option<OutputFormat>,
        name_template: &str,
        force_download: bool,
        max_tries: u32,
    ) -> anyhow::Result<()> {
        let mut idx = 1;

        for track_id in tracks {
            let item = match AudioItem::get_file(&self.session, *track_id).await {
                Ok(audio_item) => audio_item,
                Err(e) => bail!("Failed to get audio item: {e}"),
            };

            let output_format = output_format
                .or_else(|| try_get_format_from_file_name(name_template))
                .unwrap_or(OutputFormat::Opus);
            let extension = output_format.extension();

            let name = get_file_name(
                &item,
                name_template,
                Some(idx),
                if name_template.ends_with(&(".".to_owned() + extension)) {
                    None
                } else {
                    Some(&extension)
                },
            )
            .await;

            self.download_track_with_retry(
                &item,
                folder.join(Path::new(&name)).as_path(),
                output_format,
                force_download,
                max_tries,
            )
            .await?;

            idx += 1;
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        Ok(())
    }

    async fn download_playlist(&self, playlist_ref: SpotifyId, args: Args) -> anyhow::Result<()> {
        let plist = Playlist::get(&self.session, &playlist_ref).await.unwrap();
        println!("Downloading playlist {}", plist.name());

        let name = plist.name();
        let folder = Path::new(&name);

        create_dir_all(folder)
            .await
            .context("Failed to create playlist folder")?;

        self.download_tracks(
            plist.tracks(),
            folder,
            args.format,
            &args.name_format,
            args.force_download,
            args.max_tries,
        )
        .await
    }

    async fn download_album(&self, playlist_ref: SpotifyId, args: Args) -> anyhow::Result<()> {
        let album = Album::get(&self.session, &playlist_ref).await.unwrap();

        let artists = album
            .artists
            .iter()
            .map(|artist| &*artist.name)
            .collect::<Vec<&str>>()
            .join(", ");

        let folder_name = format!("{} - {}", artists, album.name);
        let folder = Path::new(&folder_name);

        create_dir_all(folder)
            .await
            .context("Failed to create album folder")?;

        println!("Downloading album {} by {}", album.name, artists);

        self.download_tracks(
            album.tracks(),
            folder,
            args.format,
            &args.name_format,
            args.force_download,
            args.max_tries,
        )
        .await
    }

    async fn download_show(&self, playlist_ref: SpotifyId, args: Args) -> anyhow::Result<()> {
        let show = Show::get(&self.session, &playlist_ref).await.unwrap();
        println!("Downloading show {} by {}", show.name, show.publisher);

        let folder = Path::new(&show.name);

        create_dir_all(folder)
            .await
            .context("Failed to create show folder")?;

        self.download_tracks(
            show.episodes.iter(),
            folder,
            args.format,
            &args.name_format,
            args.force_download,
            args.max_tries,
        )
        .await
    }

    async fn download_single_track(
        &self,
        item_ref: SpotifyId,
        path: Option<&Path>,
        output_format: Option<OutputFormat>,
        name_template: &str,
        force_download: bool,
        max_tries: u32,
    ) -> anyhow::Result<()> {
        let item = match AudioItem::get_file(&self.session, item_ref).await {
            Ok(audio_item) => audio_item,
            Err(e) => bail!("Failed to get audio item: {e}"),
        };

        let output_format = output_format
            .or_else(|| try_get_format_from_path(path))
            .or_else(|| try_get_format_from_file_name(name_template))
            .unwrap_or(OutputFormat::Opus);

        match path {
            Some(path) => {
                self.download_track_with_retry(
                    &item,
                    path,
                    output_format,
                    force_download,
                    max_tries,
                )
                .await
            }
            None => {
                let extension = output_format.extension();
                let name = get_file_name(
                    &item,
                    name_template,
                    None,
                    if name_template.ends_with(&(".".to_owned() + extension)) {
                        None
                    } else {
                        Some(&extension)
                    },
                )
                .await;
                self.download_track_with_retry(
                    &item,
                    Path::new(&name),
                    output_format,
                    force_download,
                    max_tries,
                )
                .await
            }
        }
    }

    pub async fn download(&self, item_ref: SpotifyId, args: Args) {
        if let Some(filter) = &args.cleanup_regex {
            match Regex::new(filter) {
                Ok(re) => {
                    _ = REGEX_FILTER.try_insert(re).unwrap();
                }
                Err(e) => {
                    log::warn!("Invalid regex filter: {}", e);
                }
            };
        }

        let path = args.output_path.as_ref().map(|a| a.as_path());

        if let Err(e) = match item_ref.item_type {
            SpotifyItemType::Track => {
                self.download_single_track(
                    item_ref,
                    path,
                    args.format,
                    &args.name_format,
                    args.force_download,
                    args.max_tries,
                )
                .await
            }
            SpotifyItemType::Album => self.download_album(item_ref, args).await,
            SpotifyItemType::Playlist => self.download_playlist(item_ref, args).await,
            SpotifyItemType::Episode => {
                self.download_single_track(
                    item_ref,
                    path,
                    args.format,
                    &args.name_format,
                    args.force_download,
                    args.max_tries,
                )
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

use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use anyhow::bail;
use clap::{Parser, Subcommand, command};
use librespot::{
    core::{
        Session, SessionConfig, SpotifyId, cache::Cache, error::ErrorKind,
        spotify_id::SpotifyItemType,
    },
    metadata::{Album, Metadata, Playlist, Show, audio::AudioItem},
};
use regex::Regex;

use crate::{
    core::{Loader, OutputFormat},
    metadata::get_file_name,
    playlist::{SavedPlaylist, SavedTrack},
};

mod auth;
mod core;
mod metadata;
mod playlist;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Parser)]
struct CommonArgs {
    /// Share link or Spotify URI for the downloaded item
    url: String,

    /// Location for downloaded content
    output_path: Option<PathBuf>,
}

#[derive(Parser)]
struct DownloadArgs {
    #[clap(flatten)]
    common_args: CommonArgs,

    /// Output audio format
    #[arg(short, long, value_enum, default_value = None)]
    format: Option<OutputFormat>,

    /// Format used for file names. Supports these arguments:
    /// %a - main artist name
    /// %A - all artist names separated with commas
    /// %t - track title
    /// %n - track number
    #[arg(short, long, verbatim_doc_comment, default_value = "%a - %t")]
    name_format: String,

    /// Any characters captured by this regex will be removed
    /// from the file name
    #[arg(short = 'r', long)]
    cleanup_regex: Option<String>,

    /// Always download tracks even if they already exist
    #[arg(long = "force", default_value_t = false)]
    force_download: bool,

    /// Maximum number of retries for failed requests
    #[arg(long, default_value_t = 3)]
    max_tries: u32,
}

#[derive(Parser)]
struct SaveArgs {
    #[clap(flatten)]
    common_args: CommonArgs,

    /// Saved playlist name
    #[arg(short, long)]
    name: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Download an item from Spotify
    Get(DownloadArgs),
    /// Save an item to a TOML playlist
    Save(SaveArgs),
}

fn parse_item_type(item_type: &str) -> SpotifyItemType {
    match item_type {
        "track" => SpotifyItemType::Track,
        "album" => SpotifyItemType::Album,
        "playlist" => SpotifyItemType::Playlist,
        "episode" => SpotifyItemType::Episode,
        "show" => SpotifyItemType::Show,
        _ => {
            log::warn!("Invalid item type: {}, assuming track", item_type);
            SpotifyItemType::Track
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args = Cli::parse();

    let cache = match Cache::new(
        Some("./daytrip-cache"),
        Some("./daytrip-cache"),
        Some("./daytrip-cache/audio"),
        // Audio cache is useful for streaming but we're downloading anyway, set limit to 0B
        Some(0),
    ) {
        Ok(cache) => cache,
        Err(e) => {
            bail!("Failed to open cache: {e}");
        }
    };

    let credentials = match cache.credentials() {
        Some(credentials) => {
            log::info!("Using cached credentials");
            credentials
        }
        None => {
            let credentials = match auth::get_credentials() {
                Ok(credentials) => credentials,
                Err(e) => {
                    bail!("Error getting credentials from Spotify: {e}");
                }
            };
            credentials
        }
    };

    let session = Session::new(SessionConfig::default(), Some(cache));

    match session.connect(credentials, true).await {
        Ok(_) => {}
        Err(e) => {
            if e.kind == ErrorKind::PermissionDenied {
                // Credentials might be invalid, get new ones and try again
                let credentials = match auth::get_credentials() {
                    Ok(credentials) => credentials,
                    Err(e) => {
                        bail!("Error getting credentials from Spotify: {e}");
                    }
                };
                if let Err(e) = session.connect(credentials, true).await {
                    bail!("Failed to connect to Spotify: {e}");
                }
            } else {
                bail!("Failed to connect to Spotify: {e}");
            }
        }
    }

    let loader = Loader::new(session);

    match args.command {
        Commands::Get(cmd) => {
            download(&loader, cmd).await?;
        }
        Commands::Save(cmd) => {
            save_to_file(&loader, cmd).await?;
        }
    }

    println!("All set!");

    Ok(())
}

async fn download(loader: &Loader, cmd: DownloadArgs) -> anyhow::Result<()> {
    let path = Path::new(&cmd.common_args.url);
    match File::open(path) {
        Ok(mut file) => {
            let mut buf = String::new();
            file.read_to_string(&mut buf)?;
            let plist: SavedPlaylist = toml::from_str(&buf)?;

            let folder_path = match cmd.common_args.output_path {
                Some(path) => path,
                None => PathBuf::from(&plist.title),
            };

            let format = cmd.format.unwrap_or(OutputFormat::Opus);
            let extension = format.extension();

            let mut idx = 1;

            for track in &plist.tracks {
                if let Ok(id) = track.id() {
                    let session = loader.get_session();
                    let audio_item = AudioItem::get_file(session, id).await?;

                    let file_name = match track.name() {
                        Some(name) => name.to_owned() + "." + extension,
                        None => {
                            get_file_name(&audio_item, &cmd.name_format, Some(idx), Some(extension))
                                .await
                        }
                    };

                    loader
                        .download_track_with_retry(
                            &audio_item,
                            folder_path.join(&file_name).as_path(),
                            format,
                            cmd.force_download,
                            cmd.max_tries,
                        )
                        .await?;
                }

                idx += 1;
            }
        }
        Err(_) => {
            let item_ref = if cmd.common_args.url.starts_with("spotify:") {
                let Ok(item_ref) = SpotifyId::from_base62(&cmd.common_args.url) else {
                    bail!("Invalid Spotify ID: {}", &cmd.common_args.url);
                };
                item_ref
            } else {
                let re = Regex::new(r"spotify\.com/(\w+)/(\w+)").unwrap();
                if let Some(res) = re.captures(&cmd.common_args.url) {
                    let item_type = &res[1];
                    let id = &res[2];
                    let Ok(mut item_ref) = SpotifyId::from_base62(id) else {
                        bail!("Invalid Spotify ID: {}", id);
                    };
                    item_ref.item_type = parse_item_type(item_type);
                    item_ref
                } else {
                    let Ok(mut item_ref) = SpotifyId::from_base62(&cmd.common_args.url) else {
                        bail!("Invalid Spotify ID: {}", &cmd.common_args.url);
                    };
                    item_ref.item_type = SpotifyItemType::Track;
                    item_ref
                }
            };
            loader.download(item_ref, cmd).await;
        }
    };

    tokio::fs::remove_file("temp.pcm").await?;

    Ok(())
}

async fn save_to_file(loader: &Loader, cmd: SaveArgs) -> anyhow::Result<()> {
    let item_ref = if cmd.common_args.url.starts_with("spotify:") {
        let Ok(item_ref) = SpotifyId::from_base62(&cmd.common_args.url) else {
            bail!("Invalid Spotify ID: {}", &cmd.common_args.url);
        };
        item_ref
    } else {
        let re = Regex::new(r"spotify\.com/(\w+)/(\w+)").unwrap();
        if let Some(res) = re.captures(&cmd.common_args.url) {
            let item_type = &res[1];
            let id = &res[2];
            let Ok(mut item_ref) = SpotifyId::from_base62(id) else {
                bail!("Invalid Spotify ID: {}", id);
            };
            item_ref.item_type = parse_item_type(item_type);
            item_ref
        } else {
            let Ok(mut item_ref) = SpotifyId::from_base62(&cmd.common_args.url) else {
                bail!("Invalid Spotify ID: {}", &cmd.common_args.url);
            };
            item_ref.item_type = SpotifyItemType::Track;
            item_ref
        }
    };

    let title: String;

    let tracks = match item_ref.item_type {
        SpotifyItemType::Album => {
            let session = loader.get_session();
            let plist = Album::get(session, &item_ref).await?;
            title = cmd.name.unwrap_or(plist.name.to_owned());
            plist
                .tracks()
                .filter_map(|id| match id.to_uri() {
                    Ok(id) => Some(SavedTrack::Id(id)),
                    Err(err) => {
                        log::error!("Failed to get track URI: {}", err);
                        None
                    }
                })
                .collect::<Vec<_>>()
        }
        SpotifyItemType::Episode => {
            let session = loader.get_session();
            let audio_item = AudioItem::get_file(session, item_ref).await?;
            title = cmd.name.unwrap_or(audio_item.name);
            vec![SavedTrack::Id(audio_item.uri)]
        }
        SpotifyItemType::Playlist => {
            let session = loader.get_session();
            let plist = Playlist::get(session, &item_ref).await?;
            title = cmd.name.unwrap_or(plist.name().to_owned());
            plist
                .tracks()
                .filter_map(|id| match id.to_uri() {
                    Ok(id) => Some(SavedTrack::Id(id)),
                    Err(err) => {
                        log::error!("Failed to get track URI: {}", err);
                        None
                    }
                })
                .collect::<Vec<_>>()
        }
        SpotifyItemType::Show => {
            let session = loader.get_session();
            let plist = Show::get(session, &item_ref).await?;
            title = cmd.name.unwrap_or(plist.name.to_owned());
            plist
                .episodes
                .iter()
                .filter_map(|id| match id.to_uri() {
                    Ok(id) => Some(SavedTrack::Id(id)),
                    Err(err) => {
                        log::error!("Failed to get track URI: {}", err);
                        None
                    }
                })
                .collect::<Vec<_>>()
        }
        SpotifyItemType::Track => {
            let session = loader.get_session();
            let audio_item = AudioItem::get_file(session, item_ref).await?;
            title = cmd.name.unwrap_or(audio_item.name);
            vec![SavedTrack::Id(audio_item.uri)]
        }
        _ => bail!("Unsupported item type: {:?}", item_ref.item_type),
    };

    let mut file = File::create(
        cmd.common_args
            .output_path
            .unwrap_or(PathBuf::from(&format!(
                "{}.toml",
                &metadata::legalize_name(&title)
            ))),
    )?;

    let plist = SavedPlaylist { title, tracks };

    let serialized = toml::to_string_pretty(&plist)?;

    file.write(serialized.as_bytes())?;

    Ok(())
}

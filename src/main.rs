use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use clap::{Parser, command};
use download::Loader;
use librespot::core::{
    Session, SessionConfig, SpotifyId, cache::Cache, error::ErrorKind, spotify_id::SpotifyItemType,
};
use regex::Regex;

use crate::{download::OutputFormat, metadata::get_file_name, playlist::Playlist};

mod auth;
mod download;
mod metadata;
mod playlist;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Share link or Spotify URI for the downloaded item
    url: String,

    /// Location for downloaded content
    output_path: Option<PathBuf>,

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
    let args = Args::parse();

    let cache = Cache::new(
        Some("./daytrip-cache"),
        Some("./daytrip-cache"),
        Some("./daytrip-cache/audio"),
        // Cache is useful for streaming but we're downloading anyway
        Some(0),
    )
    .unwrap_or_else(|e| {
        log::error!("Failed to open cache: {e}");
        std::process::exit(1);
    });

    let credentials = match cache.credentials() {
        Some(credentials) => {
            log::info!("Using cached credentials");
            credentials
        }
        None => {
            let credentials = match auth::get_credentials() {
                Ok(credentials) => credentials,
                Err(e) => {
                    log::error!("Error getting credentials from Spotify: {e}");
                    std::process::exit(1);
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
                        log::error!("Error getting credentials from Spotify: {e}");
                        std::process::exit(1);
                    }
                };
                if let Err(e) = session.connect(credentials, true).await {
                    log::error!("Failed to connect to Spotify: {e}");
                    std::process::exit(1);
                }
            } else {
                log::error!("Failed to connect to Spotify: {e}");
                std::process::exit(1);
            }
        }
    }

    let loader = Loader::new(session);

    let path = Path::new(&args.url);

    match File::open(path) {
        Ok(mut file) => {
            let mut buf = String::new();
            file.read_to_string(&mut buf)?;
            let plist: Playlist = toml::from_str(&buf)?;

            let folder_path = match args.output_path {
                Some(path) => path,
                None => PathBuf::from(&plist.title),
            };

            let format = args.format.unwrap_or(OutputFormat::Opus);
            let extension = format.extension();

            let mut idx = 1;

            for track in &plist.tracks {
                if let Ok(id) = track.id() {
                    let audio_item = loader.get_audio_item(id).await?;

                    let file_name = match track.name() {
                        Some(name) => name.to_owned() + "." + extension,
                        None => {
                            get_file_name(
                                &audio_item,
                                &args.name_format,
                                Some(idx),
                                Some(extension),
                            )
                            .await
                        }
                    };

                    loader
                        .download_track_with_retry(
                            &audio_item,
                            folder_path.join(&file_name).as_path(),
                            format,
                            args.force_download,
                            args.max_tries,
                        )
                        .await?;
                }

                idx += 1;
            }
        }
        Err(_) => {
            let item_ref = if args.url.starts_with("spotify:") {
                let Ok(item_ref) = SpotifyId::from_base62(&args.url) else {
                    log::error!("Invalid Spotify ID: {}", &args.url);
                    std::process::exit(1);
                };
                item_ref
            } else {
                let re = Regex::new(r"spotify\.com/(\w+)/(\w+)").unwrap();
                if let Some(res) = re.captures(&args.url) {
                    let item_type = &res[1];
                    let id = &res[2];
                    let Ok(mut item_ref) = SpotifyId::from_base62(id) else {
                        log::error!("Invalid Spotify ID: {}", id);
                        std::process::exit(1);
                    };
                    item_ref.item_type = parse_item_type(item_type);
                    item_ref
                } else {
                    let Ok(mut item_ref) = SpotifyId::from_base62(&args.url) else {
                        log::error!("Invalid Spotify ID: {}", &args.url);
                        std::process::exit(1);
                    };
                    item_ref.item_type = SpotifyItemType::Track;
                    item_ref
                }
            };
            loader.download(item_ref, args).await;
        }
    };

    tokio::fs::remove_file("temp.pcm").await?;

    println!("All set!");

    Ok(())
}

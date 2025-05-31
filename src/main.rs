use clap::{Parser, command};
use download::Loader;
use librespot::core::{
    Session, SessionConfig, SpotifyId, cache::Cache, error::ErrorKind, spotify_id::SpotifyItemType,
};
use regex::Regex;
use serde::Serialize;

mod auth;
mod download;

#[derive(clap::ValueEnum, Clone, Default, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
enum OutputFormat {
    #[default]
    Opus,
    Wav,
    Ogg,
    Mp3,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Share link or Spotify URI for the downloaded item
    url: String,

    /// Output audio format
    #[arg(short, long, value_enum, default_value_t)]
    format: OutputFormat,

    /// Remove tags like `(feat. Artist Name)` from track titles
    #[arg(short, long, default_value_t = false)]
    remove_feature_tags: bool,

    /// Add track number to file names when downloading an album or playlist
    #[arg(short, long, default_value_t = false)]
    number_tracks: bool,
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
async fn main() {
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

    loader.download(item_ref).await;

    let _ = tokio::fs::remove_file("temp.pcm").await;

    println!("All set!");
}

use download::Loader;
use librespot::core::{
    Session, SessionConfig, SpotifyId, cache::Cache, spotify_id::SpotifyItemType,
};
use regex::Regex;

mod auth;
mod download;

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
    let args: Vec<_> = std::env::args().collect();

    let Ok(credentials) = auth::get_credentials() else {
        log::error!("Error getting credentials from Spotify");
        std::process::exit(1);
    };

    let item_ref = if (&args[1]).starts_with("spotify:") {
        let Ok(item_ref) = SpotifyId::from_base62(&args[1]) else {
            log::error!("Invalid Spotify ID: {}", &args[1]);
            std::process::exit(1);
        };
        item_ref
    } else {
        let re = Regex::new(r"spotify\.com/(\w+)/(\w+)").unwrap();
        if let Some(res) = re.captures(&args[1]) {
            let item_type = &res[1];
            let id = &res[2];
            let Ok(mut item_ref) = SpotifyId::from_base62(id) else {
                log::error!("Invalid Spotify ID: {}", id);
                std::process::exit(1);
            };
            item_ref.item_type = parse_item_type(item_type);
            item_ref
        } else {
            let Ok(mut item_ref) = SpotifyId::from_base62(&args[1]) else {
                log::error!("Invalid Spotify ID: {}", &args[1]);
                std::process::exit(1);
            };
            item_ref.item_type = SpotifyItemType::Track;
            item_ref
        }
    };

    let cache = Cache::new(
        Some("./cache"),
        Some("./cache"),
        Some("./cache/audio"),
        Some(16000000),
    )
    .unwrap_or_else(|e| {
        log::error!("Failed to create cache: {e}");
        std::process::exit(1);
    });

    let session = Session::new(SessionConfig::default(), Some(cache));

    session
        .connect(credentials, true)
        .await
        .unwrap_or_else(|e| {
            log::error!("Failed to connect to Spotify: {e}");
            std::process::exit(1);
        });

    let loader = Loader::new(session);

    loader.download(item_ref).await;

    let _ = tokio::fs::remove_file("temp.pcm").await;

    println!("All set!");
}

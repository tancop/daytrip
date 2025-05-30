use librespot::{
    core::{
        Session, SessionConfig, SpotifyId, authentication::Credentials, cache::Cache,
        spotify_id::SpotifyItemType,
    },
    metadata::audio::{AudioItem, UniqueFields},
    playback::{
        audio_backend,
        config::{AudioFormat, PlayerConfig},
        mixer::NoOpVolume,
        player::Player,
    },
};
use librespot_oauth::OAuthClientBuilder;

/// Spotify's Desktop app uses these. Some of these are only available when requested with Spotify's client IDs.
static OAUTH_SCOPES: &[&str] = &[
    "app-remote-control",
    "playlist-modify",
    "playlist-modify-private",
    "playlist-modify-public",
    "playlist-read",
    "playlist-read-collaborative",
    "playlist-read-private",
    "streaming",
    "ugc-image-upload",
    "user-follow-modify",
    "user-follow-read",
    "user-library-modify",
    "user-library-read",
    "user-modify",
    "user-modify-playback-state",
    "user-modify-private",
    "user-personalized",
    "user-read-birthdate",
    "user-read-currently-playing",
    "user-read-email",
    "user-read-play-history",
    "user-read-playback-position",
    "user-read-playback-state",
    "user-read-private",
    "user-read-recently-played",
    "user-top-read",
];

const KEYMASTER_CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";
const ANDROID_CLIENT_ID: &str = "9a8d2f0ce77a4e248bb71fefcb557637";
const IOS_CLIENT_ID: &str = "58bd3c95768941ea9eb4350aaa033eb3";

#[tokio::main]
async fn main() {
    env_logger::init();
    let args: Vec<_> = std::env::args().collect();

    let oauth_port = Some(5907u16);

    let port_str = match oauth_port {
        Some(port) => format!(":{port}"),
        _ => String::new(),
    };

    let client_id = match std::env::consts::OS {
        "android" => ANDROID_CLIENT_ID,
        "ios" => IOS_CLIENT_ID,
        _ => KEYMASTER_CLIENT_ID,
    };

    let client = OAuthClientBuilder::new(
        client_id,
        &format!("http://127.0.0.1{port_str}/login"),
        OAUTH_SCOPES.to_vec(),
    )
    .open_in_browser()
    .build()
    .unwrap_or_else(|e| {
        log::error!("Failed to create OAuth client: {e}");
        std::process::exit(1);
    });

    let oauth_token = client.get_access_token().unwrap_or_else(|e| {
        log::error!("Failed to get Spotify access token: {e}");
        std::process::exit(1);
    });

    log::debug!(
        "Got access token: {}, expires: {} ms",
        oauth_token.access_token,
        oauth_token.expires_at.elapsed().as_millis()
    );

    let credentials = Credentials::with_access_token(oauth_token.access_token);

    let mut track_ref = SpotifyId::from_base62(&args[1]).unwrap();
    track_ref.item_type = SpotifyItemType::Track;

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

    let backend = audio_backend::find(Some("pipe".to_owned())).unwrap();

    let track_name: String;

    if let Ok(audio_item) = AudioItem::get_file(&session, track_ref).await {
        match audio_item.unique_fields {
            UniqueFields::Track { artists, .. } => {
                // music
                println!(
                    "Downloading {} by {}",
                    audio_item.name,
                    artists
                        .iter()
                        .map(|artist| &*artist.name)
                        .collect::<Vec<&str>>()
                        .join(", ")
                );
                track_name = format!(
                    "{} - {}.opus",
                    artists
                        .iter()
                        .map(|artist| &*artist.name)
                        .collect::<Vec<&str>>()
                        .join(", "),
                    audio_item.name
                );
            }
            UniqueFields::Episode { show_name, .. } => {
                // podcast
                println!("Downloading {} from {}", audio_item.name, show_name);
                track_name = format!("{} - {}.opus", show_name, audio_item.name);
            }
        };
    } else {
        track_name = format!("{}.opus", &args[1]);
    }

    let player = Player::new(
        PlayerConfig::default(),
        session,
        Box::new(NoOpVolume),
        move || backend(Some("audio.bin".into()), AudioFormat::S16),
    );

    player.load(track_ref, true, 0);

    player.await_end_of_track().await;

    // Read track as stereo signed 16-bit PCM and encode into a opus file
    let mut cmd = tokio::process::Command::new("ffmpeg")
        .arg("-y")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-f")
        .arg("s16le")
        .arg("-ac")
        .arg("2")
        .arg("-i")
        .arg("audio.bin")
        .arg(track_name)
        .spawn()
        .expect("Failed to spawn ffmpeg, is it installed?");

    cmd.wait().await.unwrap_or_else(|e| {
        log::error!("Failed to wait for ffmpeg: {}", e);
        std::process::exit(1);
    });

    std::fs::remove_file("audio.bin").unwrap_or_else(|e| {
        log::error!("Failed to remove audio.bin: {}", e);
    });

    println!("All set!");
}

use librespot::discovery::Credentials;
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

pub(crate) fn get_credentials() -> anyhow::Result<Credentials> {
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

    let client = match OAuthClientBuilder::new(
        client_id,
        &format!("http://127.0.0.1{port_str}/login"),
        OAUTH_SCOPES.to_vec(),
    )
    .open_in_browser()
    .build()
    {
        Ok(client) => client,
        Err(e) => {
            log::error!("Failed to create OAuth client: {e}");
            return Err(e.into());
        }
    };

    let oauth_token = match client.get_access_token() {
        Ok(token) => token,
        Err(e) => {
            log::error!("Failed to get Spotify access token: {e}");
            return Err(e.into());
        }
    };

    log::debug!(
        "Got access token: {}, expires: {} ms",
        oauth_token.access_token,
        oauth_token.expires_at.elapsed().as_millis()
    );

    let credentials = Credentials::with_access_token(oauth_token.access_token);

    Ok(credentials)
}

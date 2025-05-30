use librespot::{
    core::{Session, SpotifyId, spotify_id::SpotifyItemType},
    metadata::{
        Album, Metadata, Playlist,
        audio::{AudioItem, UniqueFields},
    },
    playback::{
        audio_backend,
        config::{AudioFormat, PlayerConfig},
        mixer::NoOpVolume,
        player::Player,
    },
};
use tokio::fs::File;

pub struct Loader {
    session: Session,
}

impl Loader {
    pub fn new(session: Session) -> Self {
        Self { session }
    }

    pub async fn download_track(&self, track_ref: SpotifyId) {
        let output_file_name: String;

        if let Ok(audio_item) = AudioItem::get_file(&self.session, track_ref).await {
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
                    output_file_name = format!(
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
                    output_file_name = format!("{} - {}.opus", show_name, audio_item.name);
                }
            };
        } else {
            output_file_name = format!("{}.opus", track_ref.to_base62().unwrap());
        }

        let backend = audio_backend::find(Some("pipe".to_owned())).unwrap();

        let player = Player::new(
            PlayerConfig::default(),
            self.session.clone(),
            Box::new(NoOpVolume),
            move || backend(Some("temp.pcm".into()), AudioFormat::S16),
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
            .arg("temp.pcm")
            .arg(output_file_name)
            .spawn()
            .expect("Failed to spawn ffmpeg, is it installed?");

        cmd.wait().await.unwrap_or_else(|e| {
            log::error!("Failed to wait for ffmpeg: {}", e);
            std::process::exit(1);
        });

        let file = match File::create("temp.pcm").await {
            Ok(file) => file,
            Err(e) => {
                log::error!("Failed to open new temp.pcm {}", e);
                return;
            }
        };
        if let Err(e) = file.set_len(0).await {
            log::error!("Failed to truncate temp.pcm: {}", e);
        }
    }

    pub async fn download_playlist(&self, playlist_ref: SpotifyId) {
        let plist = Playlist::get(&self.session, &playlist_ref).await.unwrap();
        println!("Downloading playlist {}", plist.name());
        for track_id in plist.tracks() {
            self.download_track(track_id.clone()).await;
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    pub async fn download_album(&self, playlist_ref: SpotifyId) {
        let album = Album::get(&self.session, &playlist_ref).await.unwrap();
        println!(
            "Downloading album {} by {}",
            album.name,
            album
                .artists
                .iter()
                .map(|artist| &*artist.name)
                .collect::<Vec<&str>>()
                .join(", ")
        );
        for track_id in album.tracks() {
            self.download_track(track_id.clone()).await;
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    pub async fn download(&self, item_ref: SpotifyId) {
        match item_ref.item_type {
            SpotifyItemType::Track => self.download_track(item_ref).await,
            SpotifyItemType::Album => self.download_album(item_ref).await,
            SpotifyItemType::Playlist => self.download_playlist(item_ref).await,
            SpotifyItemType::Episode => self.download_track(item_ref).await,
            _ => {
                log::error!("Unsupported item type");
                std::process::exit(1);
            }
        }
    }
}

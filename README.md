# ☀️🚌 Daytrip

Download all your music for free, forever.

## About

Daytrip uses [librespot](https://github.com/librespot-org/librespot) to download music and podcasts
directly from Spotify and save them on your device as normal audio files. No DRM, online check-ins
or proprietary formats. Your music is _yours_ again. Works for both free and premium accounts.

## Warning

Downloading music outside of the built-in premium feature is against Spotify TOS and there is a
risk of getting banned. If you don't need full premium quality you might want to use a free burner account.

## Installing

You need [ffmpeg](https://ffmpeg.org/) installed to use this. If you already have it, download the
[latest release zip](https://github.com/tancop/daytrip/releases/latest) for your system and unpack it where you want.

## Usage

- Open Spotify on any platform
- Copy the URL or a share link
- Download like this (you might need quotes around the link):

```
daytrip get https://open.spotify.com/track/1xzBco0xcoJEDXktl7Jxrr
```

By default, Daytrip downloads everything as `.opus` into the folder with its executable. You can change this with a second argument:

```
daytrip get https://open.spotify.com/track/1xzBco0xcoJEDXktl7Jxrr C:\Users\me\Music
daytrip get https://open.spotify.com/track/1xzBco0xcoJEDXktl7Jxrr song.mp3
```

### Titles

You can customize track titles with the `-n` option:

- default (`-n "%a - %t"`) -> "Playboi Carti - Love Hurts (feat. Travis Scott)"
- `-n "%A - %t"` -> "Playboi Carti, Travis Scott - Love Hurts (feat. Travis Scott)"
- `-n "%t by %a"` -> "Love Hurts (feat. Travis Scott) by Playboi Carti"
- `-n "whoa slatt"` -> "whoa slatt"

When downloading albums you might want to keep the tracks sorted with a number:

- `-n "%n %a - %t"` -> "05 Playboi Carti - Love Hurts (feat. Travis Scott)"

### Saved Playlists

You can load playlists from a TOML file instead of Spotify. This lets you customize the track list and file names:

```
daytrip get playlist.toml
```

```toml
# playlist.toml
title = "My Playlist"
tracks = [
    "spotify:track:1xzBco0xcoJEDXktl7Jxrr",
    "spotify:track:39MK3d3fonIP8Mz9oHCTBB",
    { id = "spotify:track:2JvzF1RMd7lE3KmFlsyZD8", name = "Middle Child" },
]
```

These can be manually created or exported from a Spotify URL:

```
daytrip save https://open.spotify.com/album/54Y471E7GNBSOXjZtqONId dbr.toml
```

### Title Cleanup

Some track titles come with feature tags or other stuff you don't need. You can clean them up with a regex that captures the part you want to remove:

```
daytrip get https://open.spotify.com/track/39MK3d3fonIP8Mz9oHCTBB -n "%A - %t" -r "( ?\(.*\))"
```

`Metro Boomin, Swae Lee, Lil Wayne, Offset - Annihilate (Spider-Man: Across the Spider-Verse)
(Metro Boomin & Swae Lee, Lil Wayne, Offset)` -> `Metro Boomin, Swae Lee, Lil Wayne, Offset - Annihilate`

The most useful filters for this are probably ` ?\((?:feat\.?|ft\.?|with) .+\)` to remove most feature tags and `( ?\(.+\))` to aggressively remove everything inside a pair of `( )`. If the regex gets too complicated it might be easier to save your playlist to a file and change the names one by one.

## Roadmap

- [x] Add option to remove feature tags
- [x] Add option to remove everything inside `( )` for cases like https://open.spotify.com/album/1bwbZJ6khPJyVpOaqgKsoZ
- [x] More audio formats (mp3, wav, ogg vorbis)
- [ ] Download album art
- [x] Add metadata to saved tracks
- [x] TOML playlists with custom track names
- [x] Save Spotify playlists/albums to file
- [x] Change download folder

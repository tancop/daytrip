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
daytrip https://open.spotify.com/track/1xzBco0xcoJEDXktl7Jxrr
```

By default, Daytrip downloads everything into the folder with its executable. You can change this with a second argument:

```
daytrip https://open.spotify.com/track/1xzBco0xcoJEDXktl7Jxrr C:\Users\me\Music
```

### Titles

You can customize track titles with the `-n` option:

- default (`-n "%a - %t"`) -> "Playboi Carti - Love Hurts (feat. Travis Scott)"
- `-n "%A - %t"` -> "Playboi Carti, Travis Scott - Love Hurts (feat. Travis Scott)"
- `-n "%t by %a"` -> "Love Hurts (feat. Travis Scott) by Playboi Carti"
- `-n "whoa slatt"` -> "whoa slatt"

When downloading albums you might want to keep the tracks sorted with a number:

- `-n "%n %a - %t"` -> "05 Playboi Carti - Love Hurts (feat. Travis Scott)"

### Title Cleanup

Some track titles come with feature tags or other stuff you don't need. You can clean them up with a regex that captures the part you want to remove:

```
daytrip https://open.spotify.com/track/39MK3d3fonIP8Mz9oHCTBB -n "%A - %t" -r "( ?\(.*\))"
```

`Metro Boomin, Swae Lee, Lil Wayne, Offset - Annihilate (Spider-Man: Across the Spider-Verse)
(Metro Boomin & Swae Lee, Lil Wayne, Offset)` -> `Metro Boomin, Swae Lee, Lil Wayne, Offset - Annihilate`

The most useful filters for this are probably ` ?\((?:feat\.?|ft\.?|with) .+\)` to remove some common types of feature tags and `( ?\(.+\))` to aggressively remove everything inside a pair of `( )`. If the regex gets too complicated it might be easier to download tracks one by one with a custom name.

### Options

```
Usage: daytrip [OPTIONS] <URL> [LOCATION]

Arguments:
  <URL>       Share link or Spotify URI for the downloaded item
  [LOCATION]  Location for downloaded music

Options:
  -f, --format <FORMAT>                Output audio format [default: opus] [possible values: opus, wav, ogg, mp3]
  -n, --name-format <NAME_FORMAT>      Format used for file names. Supports these arguments:
                                       %a - main artist name
                                       %A - all artist names separated with commas
                                       %t - track title
                                       %n - track number [default: "%a - %t"]
  -r, --cleanup-regex <CLEANUP_REGEX>  Any characters captured by this regex will be removed from the file name
      --force                          Always download tracks even if they already exist
      --max-tries <MAX_TRIES>          Maximum number of retries for failed requests [default: 3]
  -h, --help                           Print help
  -V, --version                        Print version
```

## Roadmap

- [x] Add option to remove feature tags
- [x] Add option to remove everything inside `( )` for cases like https://open.spotify.com/album/1bwbZJ6khPJyVpOaqgKsoZ
- [x] More audio formats (mp3, wav, ogg vorbis)
- [ ] Download album art
- [ ] Add metadata to saved tracks
- [ ] TOML playlists with custom track names
- [ ] Save Spotify playlists/albums to file
- [x] Change download folder

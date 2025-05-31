# ☀️🚌 Daytrip

Download all your music for free, forever.

## About

Daytrip uses [librespot](https://github.com/librespot-org/librespot) to download music and podcasts
directly from Spotify and save them on your device as normal audio files. No DRM, online check-ins
or proprietary formats. Your music is _yours_ again. Works for both free and premium accounts.

## Warning

Downloading music like this is against Spotify's TOS and might get you banned. If you don't need full premium
quality you should probably use a free burner account. Bypassing copy protection might be illegal in some
countries (looking at you America). **You are the only one responsible for any legal issues caused by using Daytrip**.

## Installing

You need [ffmpeg](https://ffmpeg.org/) installed to use this. You should probably download it with a package manager
like dnf, apt, pacman or winget. If you already have it, download the
[latest release zip](https://github.com/tancop/daytrip/releases/latest) for your system and unpack it where you want.

## Usage

You can download songs, albums, playlists, podcast episodes or shows using their share link.
Open the Spotify app or web player, right click on the thing you want to download and select
`Share > Copy link to [...]`. Then open a terminal in the folder with Daytrip and paste the link like this:

```
./daytrip "https://open.spotify.com/track/1xzBco0xcoJEDXktl7Jxrr?si=aff53d31ec5b405c"
```

You might not need quotes around the link on some systems. If this is your first time downloading or the cached
token is too old, Daytrip opens a browser tab asking you to authenticate with Spotify. The tab closes automatically
after that. Later downloads will try and use cached credentials to skip the login process.

### Options

```
Usage: daytrip [OPTIONS] <URL>

Arguments:
  <URL>  Share link or Spotify URI for the downloaded item

Options:
  -f, --format <FORMAT>
          Output audio format [default: opus] [possible values: opus, wav, ogg, mp3]
  -r, --remove-feature-tags
          Remove tags like `(feat. Artist Name)` from track titles
  -t, --track-title-filter <TRACK_TITLE_FILTER>
          Regular expression used to filter track titles. Any captures will be removed
  -n, --number-tracks
          Add track number to file names when downloading an album or playlist
  -m, --main-artist-only
          Only show main artist on titles if there's more than one
  -h, --help
          Print help
  -V, --version
          Print version
```

### Title Cleanup

The `-r` option is useful for cleaning up messy titles with a long feature list:

```
daytrip https://open.spotify.com/track/4X5f3vT8MRuXF68pfjNte5 -r
```

- Without filter: `A$AP Rocky, Drake, 2 Chainz, Kendrick Lamar - F**kin' Problems (feat. Drake, 2 Chainz & Kendrick Lamar)`
- With filter: `A$AP Rocky, Drake, 2 Chainz, Kendrick Lamar - F**kin' Problems`

You might need a custom regex filter with `-t` in more complicated cases, like this one to remove everything inside `( )`:

```
daytrip https://open.spotify.com/track/39MK3d3fonIP8Mz9oHCTBB -t '( ?\(.+?\))'
```

- Without filter: `Metro Boomin, Swae Lee, Lil Wayne, Offset - Annihilate (Spider-Man: Across the Spider-Verse) (Metro Boomin & Swae Lee, Lil Wayne, Offset)`
- With filter: `Metro Boomin, Swae Lee, Lil Wayne, Offset - Annihilate`

Or you can do it in reverse with `-m`:

```
daytrip https://open.spotify.com/track/4X5f3vT8MRuXF68pfjNte5 -m
```

- `A$AP Rocky - F**kin' Problems (feat. Drake, 2 Chainz & Kendrick Lamar)`

## Downloads

All downloaded music gets saved next to the `daytrip` executable as a `.opus` file with the track name and artists:

```
Lil Wayne, Cory Gunz - 6 Foot 7 Foot.opus
```

Albums and playlists download into a folder like `My Playlist` or `Drake - Views`. Podcasts work the same but
with the show title instead of artist names.

## Roadmap

- [x] Add option to remove feature tags
- [ ] Add option to remove everything inside `( )` for cases like https://open.spotify.com/album/1bwbZJ6khPJyVpOaqgKsoZ
- [x] More audio formats (mp3, wav, ogg vorbis)
- [ ] Downloading album art
- [ ] Change download folder

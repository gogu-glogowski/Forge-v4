# Forge Calm

18 calm instrumental recordings by Kevin MacLeod (incompetech.com): kalimba,
harp, piano and ambient. Measured duration: **2 h 59 min 51.5 s**; total 346.2 MiB.
Two recordings are losslessly split into two parts each (20 files total);
no audio is re-encoded. Filenames were renamed for ordering.

## Automatic online playback

Install the single Forge DEB (mpv is recommended), then run `forge prepare --all`.
When no local audio library is present, Forge streams this repository's MP3 files
through mpv over HTTPS: 100% volume, shuffle and repeat, no player window.
The source is `https://raw.githubusercontent.com/gogu-glogowski/Forge-v4/main/assets/music/`.
The playlist is embedded in the release binary from `manifest.json`; remote URLs
are not taken from user-supplied playlists. Streaming buffers audio on demand;
it does not download the full 346 MiB collection up front or keep a persistent
online-audio cache. It stops on completion, error or Ctrl+C. Network/player/audio
failures are nonfatal for image preparation. APT itself does not play music.

`--no-music` disables playback and makes no music network requests.
`--music-dir /path/to/library` or `FORGE_MUSIC_DIR` chooses local audio instead.
Otherwise Forge checks user data (`~/.local/share/forge/music` by default),
`/usr/share/forge/music`, and checkout `assets/music`, then streams online.
The DEB installs attribution files in the system directory, without MP3s.

To use music offline, copy this directory's contents into the user data music
directory. Existing local collections continue to work. Add/remove local MP3,
OGG, FLAC, WAV or M4A files without changing Rust code. For changes to the online
collection, update `manifest.json` and rebuild Forge. Each playback MP3 is below 40 MiB.

## License

These are free **CC BY 4.0** recordings, not public domain. Keep `CREDITS.md`,
`LICENSE-CC-BY-4.0.txt` and `manifest.json` with redistributed music. The Apache-2.0
software license does not replace the music license. Forge shows composer/license
attribution when streaming; the DEB includes complete credits. Per-track source
URLs, original download URLs, ISRC, duration, sizes and SHA-256 are in the manifest.
`playlist.m3u` is an optional relative playlist for manual listening.

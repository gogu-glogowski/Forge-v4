//! Optional local or repository-streamed audio. Player failures must never interrupt image preparation.
use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Playback {
    child: Option<Child>,
    directory: Option<PathBuf>,
}
impl Playback {
    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(directory) = self.directory.take() {
            let _ = fs::remove_dir_all(directory);
        }
    }
}
#[derive(Default)]
pub struct Session(Arc<Mutex<Playback>>);
impl Session {
    pub fn stopper(&self) -> impl Fn() + Send + 'static {
        let state = self.0.clone();
        move || {
            if let Ok(mut playback) = state.lock() {
                playback.stop();
            }
        }
    }
    pub fn start(&self, meta: &Path, enabled: bool, requested: Option<PathBuf>) {
        if !enabled {
            return;
        }
        let directory = requested
            .or_else(|| std::env::var_os("FORGE_MUSIC_DIR").map(PathBuf::from))
            .or_else(|| {
                [
                    meta.join("music"),
                    PathBuf::from("/usr/share/forge/music"),
                    PathBuf::from("assets/music"),
                ]
                .into_iter()
                .find(|path| !tracks(path).is_empty())
            });
        let Some(directory) = directory else {
            let files = repository_tracks();
            if let Err(error) = self.spawn(&files, Path::new("mpv")) {
                eprintln!("Music: {error}; install mpv to enable playback. Preparation continues.");
            } else {
                eprintln!(
                    "Music: streaming Forge Calm from github.com/gogu-glogowski/Forge-v4, volume 100%. Use --no-music for silence."
                );
                eprintln!(
                    "Kevin MacLeod (incompetech.com), CC BY 4.0: https://creativecommons.org/licenses/by/4.0/ — credits: /usr/share/forge/music/CREDITS.md"
                );
            }
            return;
        };
        let files = tracks(&directory);
        if files.is_empty() {
            eprintln!(
                "Music: {} contains no supported audio; continuing silently.",
                directory.display()
            );
            return;
        }
        if let Err(error) = self.spawn(&files, Path::new("mpv")) {
            eprintln!("Music: {error}; install mpv to enable playback. Preparation continues.");
            return;
        }
        eprintln!(
            "Music: {} tracks, shuffled and looping, volume 100%. Use --no-music for silence.",
            files.len()
        );
        if directory.join("CREDITS.md").is_file() {
            eprintln!("Music credits: {}", directory.join("CREDITS.md").display());
        }
        if fs::read_to_string(directory.join("manifest.json"))
            .is_ok_and(|text| text.contains("\"playlist\": \"Forge Calm\""))
        {
            eprintln!(
                "Forge Calm — Kevin MacLeod (incompetech.com), CC BY 4.0: https://creativecommons.org/licenses/by/4.0/"
            );
        }
    }
    fn spawn(&self, files: &[PathBuf], player: &Path) -> std::io::Result<()> {
        let directory = std::env::temp_dir().join(format!("forge-music-{}", uuid::Uuid::new_v4()));
        fs::DirBuilder::new().mode(0o700).create(&directory)?;
        let playlist = directory.join("playlist.m3u");
        let result = (|| {
            let mut contents = String::from("#EXTM3U\n");
            for file in files {
                contents.push_str(&file.to_string_lossy());
                contents.push('\n');
            }
            fs::write(&playlist, contents)?;
            fs::set_permissions(&playlist, fs::Permissions::from_mode(0o600))?;
            Command::new(player)
                .args([
                    "--no-config",
                    "--load-scripts=no",
                    "--ytdl=no",
                    "--no-video",
                    "--no-terminal",
                    "--really-quiet",
                    "--volume=100",
                    "--cache=yes",
                    "--network-timeout=10",
                    "--shuffle",
                    "--loop-playlist=inf",
                ])
                .arg(format!("--playlist={}", playlist.display()))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .process_group(0)
                .spawn()
        })();
        match result {
            Ok(child) => {
                let mut playback = self.0.lock().expect("music mutex");
                playback.stop();
                playback.child = Some(child);
                playback.directory = Some(directory);
                Ok(())
            }
            Err(error) => {
                let _ = fs::remove_dir_all(directory);
                Err(error)
            }
        }
    }
    pub fn poll(&self) {
        if let Ok(mut playback) = self.0.lock() {
            let finished = playback
                .child
                .as_mut()
                .is_some_and(|child| child.try_wait().ok().flatten().is_some());
            if finished {
                eprintln!(
                    "Music player stopped (check mpv/audio configuration); preparation continues."
                );
                playback.stop();
            }
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        (self.stopper())();
    }
}
// URLs come only from the release's embedded manifest, never from a user playlist.
fn repository_tracks() -> Vec<PathBuf> {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../../../assets/music/manifest.json"))
            .expect("bundled music manifest");
    manifest["tracks"]
        .as_array()
        .expect("bundled tracks")
        .iter()
        .map(|track| {
            let file = track["file"].as_str().expect("bundled filename");
            assert!(
                file.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
                    && !file.contains("..")
            );
            PathBuf::from(format!(
                "https://raw.githubusercontent.com/gogu-glogowski/Forge-v4/main/assets/music/{file}"
            ))
        })
        .collect()
}

fn tracks(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(directory) else {
        return vec![];
    };
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let extension = path.extension()?.to_str()?.to_ascii_lowercase();
            if !path.is_file()
                || !["mp3", "ogg", "flac", "wav", "m4a"].contains(&extension.as_str())
            {
                return None;
            }
            let absolute = path.canonicalize().ok()?;
            let text = absolute.to_str()?;
            if text.contains(['\n', '\r']) {
                return None;
            }
            Some(absolute)
        })
        .collect();
    files.sort();
    files
}
#[cfg(test)]
mod tests {
    use super::*;
    fn temporary() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("forge-audio-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        dir
    }
    #[test]
    fn library_only_accepts_local_audio_and_safe_absolute_paths() {
        let dir = temporary();
        for name in [
            "a.mp3",
            "b.FLAC",
            "bad\nname.mp3",
            "remote.m3u",
            "README.md",
        ] {
            fs::write(dir.join(name), "test").unwrap();
        }
        fs::create_dir(dir.join("directory.wav")).unwrap();
        let files = tracks(&dir);
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|p| p.is_absolute()));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn stopping_reaps_player_and_removes_only_internal_playlist() {
        let dir = temporary();
        let audio = dir.join("keep.mp3");
        fs::write(&audio, "audio").unwrap();
        let player = dir.join("player");
        fs::write(&player, "#!/bin/sh\nexec sleep 30\n").unwrap();
        fs::set_permissions(&player, fs::Permissions::from_mode(0o755)).unwrap();
        let session = Session::default();
        session
            .spawn(std::slice::from_ref(&audio), &player)
            .unwrap();
        let internal = session.0.lock().unwrap().directory.clone().unwrap();
        assert!(internal.join("playlist.m3u").is_file());
        (session.stopper())();
        assert!(!internal.exists());
        assert!(audio.exists());
        assert!(session.0.lock().unwrap().child.is_none());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn repository_playlist_uses_only_official_https_audio_urls() {
        let urls = repository_tracks();
        assert_eq!(urls.len(), 20);
        for url in urls {
            let text = url.to_str().unwrap();
            assert!(text.starts_with(
                "https://raw.githubusercontent.com/gogu-glogowski/Forge-v4/main/assets/music/"
            ));
            assert!(text.ends_with(".mp3"));
            assert!(!text.contains(['\n', '\r']));
        }
    }
    #[test]
    fn streaming_playlist_is_cleaned_up_like_local_playback() {
        let dir = temporary();
        let player = dir.join("player");
        fs::write(&player, "#!/bin/sh\nexec sleep 30\n").unwrap();
        fs::set_permissions(&player, fs::Permissions::from_mode(0o755)).unwrap();
        let session = Session::default();
        session.spawn(&repository_tracks(), &player).unwrap();
        let internal = session.0.lock().unwrap().directory.clone().unwrap();
        assert!(
            fs::read_to_string(internal.join("playlist.m3u"))
                .unwrap()
                .contains("https://raw.githubusercontent.com/")
        );
        drop(session);
        assert!(!internal.exists());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn missing_player_is_a_recoverable_error() {
        let session = Session::default();
        assert!(session.spawn(&[], Path::new("/no-such-forge-mpv")).is_err());
        assert!(session.0.lock().unwrap().directory.is_none());
    }
}

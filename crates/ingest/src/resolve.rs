//! Picks the input file: an explicit path, else the only video in the inbox,
//! else whatever the user answers when asked.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("no input video: pass a file or put one video in the inbox")]
    NoInput,
}

const VIDEO_EXTENSIONS: [&str; 6] = ["mp4", "mov", "m4v", "webm", "mkv", "avi"];

/// `ask` receives a question for the user and returns their answer, or
/// `None` when they gave none.
pub fn resolve(
    explicit: Option<&Path>,
    inbox: &Path,
    ask: impl FnOnce(&str) -> Option<PathBuf>,
) -> Result<PathBuf, ResolveError> {
    if let Some(path) = explicit {
        return Ok(path.to_path_buf());
    }
    let mut videos = inbox_videos(inbox);
    if videos.len() == 1 {
        return Ok(videos.remove(0));
    }
    let question = if videos.is_empty() {
        format!("No video found in {}. Path to the video:", inbox.display())
    } else {
        let names: Vec<String> = videos
            .iter()
            .map(|v| format!("  {}", v.display()))
            .collect();
        format!(
            "{} videos in {}:\n{}\nWhich one?",
            videos.len(),
            inbox.display(),
            names.join("\n")
        )
    };
    ask(&question).ok_or(ResolveError::NoInput)
}

fn inbox_videos(inbox: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(inbox) else {
        return Vec::new();
    };
    let mut videos: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| VIDEO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        })
        .collect();
    videos.sort();
    videos
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn an_explicit_path_wins_without_asking() {
        let inbox = tempfile::tempdir().unwrap();
        let picked = resolve(Some(Path::new("clip.mp4")), inbox.path(), |_| {
            panic!("must not ask")
        });
        assert_eq!(picked.unwrap(), PathBuf::from("clip.mp4"));
    }

    #[test]
    fn the_single_video_in_the_inbox_is_used_without_asking() {
        let inbox = tempfile::tempdir().unwrap();
        fs::write(inbox.path().join("hook.mp4"), b"").unwrap();
        fs::write(inbox.path().join("notes.txt"), b"").unwrap();
        let picked = resolve(None, inbox.path(), |_| panic!("must not ask"));
        assert_eq!(picked.unwrap(), inbox.path().join("hook.mp4"));
    }

    #[test]
    fn two_videos_in_the_inbox_means_ask_and_list_both() {
        let inbox = tempfile::tempdir().unwrap();
        fs::write(inbox.path().join("a.mp4"), b"").unwrap();
        fs::write(inbox.path().join("b.MOV"), b"").unwrap();
        let mut question = String::new();
        let picked = resolve(None, inbox.path(), |q| {
            question = q.to_string();
            Some(PathBuf::from("/videos/chosen.mp4"))
        });
        assert_eq!(picked.unwrap(), PathBuf::from("/videos/chosen.mp4"));
        assert!(
            question.contains("a.mp4") && question.contains("b.MOV"),
            "{question}"
        );
    }

    #[test]
    fn an_empty_or_missing_inbox_asks_and_no_answer_is_an_error() {
        let missing = Path::new("/definitely/not/here");
        let mut asked = false;
        let picked = resolve(None, missing, |_| {
            asked = true;
            None
        });
        assert!(asked);
        assert!(matches!(picked, Err(ResolveError::NoInput)));
    }
}

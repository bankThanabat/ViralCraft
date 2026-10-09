//! Checks for the tools and model files ViralCraft needs, and the commands
//! that install them. Never installs anything on its own.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};

/// External binaries ViralCraft shells out to.
pub const TOOLS: [&str; 2] = ["ffmpeg", "ffprobe"];

/// A model file pinned by URL, size and SHA-256.
#[derive(Debug, Clone, Copy)]
pub struct ModelSpec {
    pub file: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size: u64,
}

/// Every model ViralCraft loads. Hashes were taken from the files verified on
/// 2026-10-09; the whisper hash matches Hugging Face's published LFS etag.
pub const MODELS: [ModelSpec; 3] = [
    ModelSpec {
        file: "ggml-tiny.en.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin",
        sha256: "921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f",
        size: 77_704_715,
    },
    ModelSpec {
        file: "text-detection.rten",
        url: "https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.rten",
        sha256: "f15cfb56bd02c4bf478a20343986504a1f01e1665c2b3a0ad66340f054b1b5ca",
        size: 2_510_284,
    },
    ModelSpec {
        file: "text-recognition.rten",
        url: "https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten",
        sha256: "e484866d4cce403175bd8d00b128feb08ab42e208de30e42cd9889d8f1735a6e",
        size: 9_716_568,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Linux,
    Windows,
}

impl Os {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Os::MacOs
        } else if cfg!(windows) {
            Os::Windows
        } else {
            Os::Linux
        }
    }
}

/// Looks up an executable on `PATH`.
pub type FindTool<'a> = Box<dyn Fn(&str) -> Option<PathBuf> + 'a>;

/// Where to look. Injected so tests never depend on the machine they run on.
pub struct Env<'a> {
    pub os: Os,
    pub models_dir: &'a Path,
    pub find_tool: FindTool<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Present,
    Missing,
    /// The file exists but its size or SHA-256 does not match the manifest.
    Corrupt,
}

#[derive(Debug, Clone, Serialize)]
pub struct Item {
    pub name: String,
    pub state: State,
    pub path: Option<PathBuf>,
    /// The command that fixes this item; `None` when it is present.
    pub fix: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub ok: bool,
    pub items: Vec<Item>,
}

impl Report {
    pub fn item(&self, name: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.name == name)
    }
}

pub fn tool_fix(_tool: &str, os: Os) -> String {
    // ffprobe ships inside the ffmpeg package everywhere.
    match os {
        Os::MacOs => "brew install ffmpeg",
        Os::Linux => "sudo apt-get install -y ffmpeg",
        Os::Windows => "winget install --id Gyan.FFmpeg -e",
    }
    .to_string()
}

fn model_fix(model: &ModelSpec, path: &Path) -> String {
    format!(
        "curl -fL --create-dirs -o \"{}\" {}",
        path.display(),
        model.url
    )
}

/// The default models directory: `~/.cache/viralcraft/models`, or
/// `$VIRALCRAFT_MODELS` when set.
pub fn default_models_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("VIRALCRAFT_MODELS") {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join(".cache").join("viralcraft").join("models")
}

pub fn check(env: &Env, models: &[ModelSpec]) -> Report {
    let mut items = Vec::new();
    for tool in TOOLS {
        let path = (env.find_tool)(tool);
        items.push(Item {
            name: tool.to_string(),
            state: if path.is_some() {
                State::Present
            } else {
                State::Missing
            },
            fix: path.is_none().then(|| tool_fix(tool, env.os)),
            path,
        });
    }
    for model in models {
        let path = env.models_dir.join(model.file);
        let state = model_state(model, &path);
        items.push(Item {
            name: model.file.to_string(),
            state,
            fix: (state != State::Present).then(|| model_fix(model, &path)),
            path: Some(path),
        });
    }
    let ok = items.iter().all(|i| i.state == State::Present);
    Report { ok, items }
}

fn model_state(model: &ModelSpec, path: &Path) -> State {
    let Ok(meta) = path.metadata() else {
        return State::Missing;
    };
    if meta.len() != model.size {
        return State::Corrupt;
    }
    match sha256_file(path) {
        Ok(hash) if hash == model.sha256 => State::Present,
        _ => State::Corrupt,
    }
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut hasher = Sha256::new();
    io::copy(&mut File::open(path)?, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// Offers each fix in turn. `ask` returns whether the user accepted; `run`
/// executes the command. A command shared by several items runs once.
pub fn install<E>(
    report: &Report,
    mut ask: impl FnMut(&Item) -> bool,
    mut run: impl FnMut(&str) -> Result<(), E>,
) -> Result<Vec<String>, E> {
    let mut ran: Vec<String> = Vec::new();
    for item in &report.items {
        let Some(fix) = &item.fix else { continue };
        if ran.contains(fix) {
            continue;
        }
        if ask(item) {
            run(fix)?;
            ran.push(fix.clone());
        }
    }
    Ok(ran)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn sha256_hex(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(bytes))
    }

    fn test_model(name: &'static str, content: &[u8]) -> ModelSpec {
        ModelSpec {
            file: name,
            url: "https://example.test/model.bin",
            sha256: Box::leak(sha256_hex(content).into_boxed_str()),
            size: content.len() as u64,
        }
    }

    fn env<'a>(os: Os, models_dir: &'a Path, found: &'a [&'a str]) -> Env<'a> {
        Env {
            os,
            models_dir,
            find_tool: Box::new(move |name| {
                found
                    .contains(&name)
                    .then(|| PathBuf::from(format!("/usr/bin/{name}")))
            }),
        }
    }

    #[test]
    fn missing_tools_are_reported_with_the_brew_command_on_macos() {
        let dir = tempfile::tempdir().unwrap();
        let report = check(&env(Os::MacOs, dir.path(), &[]), &[]);

        assert!(!report.ok);
        let ffmpeg = report.item("ffmpeg").unwrap();
        assert_eq!(ffmpeg.state, State::Missing);
        assert_eq!(ffmpeg.fix.as_deref(), Some("brew install ffmpeg"));
        assert_eq!(report.item("ffprobe").unwrap().state, State::Missing);
    }

    #[test]
    fn install_commands_follow_the_operating_system() {
        assert_eq!(
            tool_fix("ffmpeg", Os::Linux),
            "sudo apt-get install -y ffmpeg"
        );
        assert_eq!(
            tool_fix("ffprobe", Os::Windows),
            "winget install --id Gyan.FFmpeg -e"
        );
    }

    #[test]
    fn a_model_with_the_wrong_hash_is_corrupt_not_present() {
        let dir = tempfile::tempdir().unwrap();
        let model = test_model("m.bin", b"expected");
        fs::write(dir.path().join("m.bin"), b"tampered").unwrap();

        let report = check(
            &env(Os::MacOs, dir.path(), &["ffmpeg", "ffprobe"]),
            &[model],
        );

        assert_eq!(report.item("m.bin").unwrap().state, State::Corrupt);
        assert!(!report.ok);
    }

    #[test]
    fn a_missing_model_gets_a_curl_download_command_into_the_models_dir() {
        let dir = tempfile::tempdir().unwrap();
        let model = test_model("m.bin", b"weights");

        let report = check(
            &env(Os::MacOs, dir.path(), &["ffmpeg", "ffprobe"]),
            &[model],
        );

        let item = report.item("m.bin").unwrap();
        assert_eq!(item.state, State::Missing);
        let fix = item.fix.as_deref().unwrap();
        assert!(fix.starts_with("curl -fL --create-dirs -o "), "{fix}");
        assert!(fix.contains(&dir.path().join("m.bin").display().to_string()));
        assert!(fix.ends_with("https://example.test/model.bin"));
    }

    #[test]
    fn everything_present_and_valid_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        let model = test_model("m.bin", b"weights");
        fs::write(dir.path().join("m.bin"), b"weights").unwrap();

        let report = check(
            &env(Os::MacOs, dir.path(), &["ffmpeg", "ffprobe"]),
            &[model],
        );

        assert!(report.ok);
        assert!(report
            .items
            .iter()
            .all(|i| i.state == State::Present && i.fix.is_none()));
        assert_eq!(
            report.item("ffmpeg").unwrap().path.as_deref(),
            Some(Path::new("/usr/bin/ffmpeg"))
        );
    }

    #[test]
    fn install_asks_per_item_and_runs_only_the_accepted_ones() {
        let dir = tempfile::tempdir().unwrap();
        let report = check(&env(Os::MacOs, dir.path(), &[]), &[]);

        let mut asked = Vec::new();
        let mut ran = Vec::new();
        install(
            &report,
            |item| {
                asked.push(item.name.clone());
                item.name == "ffprobe"
            },
            |cmd| {
                ran.push(cmd.to_string());
                Ok::<(), std::convert::Infallible>(())
            },
        )
        .unwrap();

        assert_eq!(asked, ["ffmpeg", "ffprobe"]);
        assert_eq!(ran, ["brew install ffmpeg"]);
    }

    #[test]
    fn the_shipped_manifest_pins_three_models_with_real_hashes() {
        let names: Vec<_> = MODELS.iter().map(|m| m.file).collect();
        assert_eq!(
            names,
            [
                "ggml-tiny.en.bin",
                "text-detection.rten",
                "text-recognition.rten"
            ]
        );
        assert!(MODELS
            .iter()
            .all(|m| m.sha256.len() == 64 && m.url.starts_with("https://")));
    }
}

//! Checks for the tools and model files ViralCraft needs, and the commands
//! that install them. Never installs anything on its own.

pub mod setup;

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
    /// Optional models enable an extra feature; missing is fine.
    pub optional: bool,
    /// What the model is for, shown when asking to install it.
    pub purpose: &'static str,
    /// Interchangeable models offered as one choice; any one is enough.
    pub group: Option<&'static str>,
}

/// The multilingual whisper models: pick one size.
pub const MULTILINGUAL: &str = "speech in other languages";

/// Every model ViralCraft loads. Hashes were taken from the files verified on
/// 2026-10-09; the whisper and YAMNet hashes match Hugging Face's published
/// LFS etags. YAMNet is Google's AudioSet sound classifier (Apache-2.0),
/// converted to ONNX by the audiomagic repo; the URL is pinned to a commit.
pub const MODELS: [ModelSpec; 7] = [
    ModelSpec {
        file: "ggml-tiny.en.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin",
        sha256: "921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f",
        size: 77_704_715,
        optional: false,
        purpose: "speech transcription",
        group: None,
    },
    ModelSpec {
        file: "text-detection.rten",
        url: "https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.rten",
        sha256: "f15cfb56bd02c4bf478a20343986504a1f01e1665c2b3a0ad66340f054b1b5ca",
        size: 2_510_284,
        optional: false,
        purpose: "finding on-screen text",
        group: None,
    },
    ModelSpec {
        file: "text-recognition.rten",
        url: "https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten",
        sha256: "e484866d4cce403175bd8d00b128feb08ab42e208de30e42cd9889d8f1735a6e",
        size: 9_716_568,
        optional: false,
        purpose: "reading on-screen text",
        group: None,
    },
    ModelSpec {
        file: "yamnet.onnx",
        url: "https://huggingface.co/audiomagic/yamnet-onnx/resolve/f25b741c2f0bdc6d7e6db24b5fddda23347dbafd/yamnet.onnx",
        sha256: "d3835ffbbd4a1bb3e777f0ca217b5007907f5171dd5d17c4236b95b2af8f908e",
        size: 16_093_355,
        optional: true,
        purpose: "sound-effect names (typing, whoosh, pop) in the transcript",
        group: None,
    },
    ModelSpec {
        file: "ggml-small.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-small.bin",
        sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
        size: 487_601_967,
        optional: true,
        purpose: "speech in other languages: small (0.5 GB, good)",
        group: Some(MULTILINGUAL),
    },
    ModelSpec {
        file: "ggml-medium.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-medium.bin",
        sha256: "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208",
        size: 1_533_763_059,
        optional: true,
        purpose: "speech in other languages: medium (1.5 GB, better)",
        group: Some(MULTILINGUAL),
    },
    ModelSpec {
        file: "ggml-large-v3.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v3.bin",
        sha256: "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2",
        size: 3_095_033_483,
        optional: true,
        purpose: "speech in other languages: large (3.1 GB, best)",
        group: Some(MULTILINGUAL),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Tool,
    Model,
}

#[derive(Debug, Clone, Serialize)]
pub struct Item {
    pub name: String,
    pub kind: Kind,
    pub state: State,
    pub path: Option<PathBuf>,
    /// The command that fixes this item; `None` when it is present.
    pub fix: Option<String>,
    /// Missing optional items do not fail the check.
    pub optional: bool,
    pub purpose: String,
    /// Set for interchangeable models offered as one choice.
    pub group: Option<String>,
    /// Download size in bytes; `None` for tools.
    pub size: Option<u64>,
}

impl Item {
    /// Whether this item fails the check: anything not present, except a
    /// missing optional extra.
    pub fn fails(&self) -> bool {
        self.state != State::Present && !(self.optional && self.state == State::Missing)
    }
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

    /// Whether anything could be installed, including optional extras that
    /// do not fail the check.
    pub fn has_offers(&self) -> bool {
        self.items
            .iter()
            .any(|i| i.fix.is_some() && !self.group_satisfied(i.group.as_deref()))
    }

    /// Whether some member of `group` is installed (always false for none).
    pub fn group_satisfied(&self, group: Option<&str>) -> bool {
        group.is_some_and(|g| {
            self.items
                .iter()
                .any(|i| i.group.as_deref() == Some(g) && i.state == State::Present)
        })
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
            kind: Kind::Tool,
            state: if path.is_some() {
                State::Present
            } else {
                State::Missing
            },
            fix: path.is_none().then(|| tool_fix(tool, env.os)),
            path,
            optional: false,
            purpose: "video and audio decoding".to_string(),
            group: None,
            size: None,
        });
    }
    for model in models {
        let path = env.models_dir.join(model.file);
        let state = model_state(model, &path);
        items.push(Item {
            name: model.file.to_string(),
            kind: Kind::Model,
            state,
            fix: (state != State::Present).then(|| model_fix(model, &path)),
            path: Some(path),
            optional: model.optional,
            purpose: model.purpose.to_string(),
            group: model.group.map(str::to_string),
            size: Some(model.size),
        });
    }
    let ok = !items.iter().any(Item::fails);
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
            optional: false,
            purpose: "test",
            group: None,
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
    fn setup_has_nothing_to_ask_when_every_file_matches_its_hash() {
        let dir = tempfile::tempdir().unwrap();
        let models = [
            test_model("speech.bin", b"speech"),
            optional_model("sfx.onnx", b"sfx"),
            grouped("s.bin", b"s"),
        ];
        fs::write(dir.path().join("speech.bin"), b"speech").unwrap();
        fs::write(dir.path().join("sfx.onnx"), b"sfx").unwrap();
        fs::write(dir.path().join("s.bin"), b"s").unwrap();

        let report = check(&env(Os::MacOs, dir.path(), &["ffmpeg", "ffprobe"]), &models);

        assert!(report.ok);
        assert_eq!(
            setup::plan(&report, Os::MacOs, false),
            [],
            "nothing to install, so brew does not matter"
        );
    }

    fn grouped(name: &'static str, content: &[u8]) -> ModelSpec {
        ModelSpec {
            optional: true,
            group: Some("multilingual speech"),
            purpose: "speech in other languages",
            ..test_model(name, content)
        }
    }

    #[test]
    fn a_group_with_one_member_installed_offers_nothing() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("m.bin"), b"m").unwrap();
        let models = [
            grouped("s.bin", b"s"),
            grouped("m.bin", b"m"),
            grouped("l.bin", b"l"),
        ];
        let report = check(&env(Os::MacOs, dir.path(), &["ffmpeg", "ffprobe"]), &models);
        assert!(!report.has_offers());
    }

    fn optional_model(name: &'static str, content: &[u8]) -> ModelSpec {
        ModelSpec {
            optional: true,
            purpose: "sound-effect names",
            ..test_model(name, content)
        }
    }

    #[test]
    fn a_missing_optional_model_does_not_fail_the_check() {
        let dir = tempfile::tempdir().unwrap();
        let report = check(
            &env(Os::MacOs, dir.path(), &["ffmpeg", "ffprobe"]),
            &[optional_model("sfx.onnx", b"w")],
        );
        assert!(report.ok);
        let item = report.item("sfx.onnx").unwrap();
        assert_eq!(item.state, State::Missing);
        assert!(item.optional);
        assert!(item.fix.is_some(), "still offered");
    }

    #[test]
    fn install_is_offered_when_only_an_optional_model_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let report = check(
            &env(Os::MacOs, dir.path(), &["ffmpeg", "ffprobe"]),
            &[optional_model("sfx.onnx", b"w")],
        );
        assert!(report.ok);
        assert!(report.has_offers());
    }

    #[test]
    fn nothing_is_offered_when_everything_is_present() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!check(&env(Os::MacOs, dir.path(), &["ffmpeg", "ffprobe"]), &[]).has_offers());
    }

    #[test]
    fn a_corrupt_optional_model_does_fail_the_check() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("sfx.onnx"), b"tampered").unwrap();
        let report = check(
            &env(Os::MacOs, dir.path(), &["ffmpeg", "ffprobe"]),
            &[optional_model("sfx.onnx", b"w")],
        );
        assert!(!report.ok, "a broken file would crash the feature");
    }

    #[test]
    fn the_shipped_manifest_pins_every_model_with_a_real_hash() {
        let names: Vec<_> = MODELS.iter().map(|m| m.file).collect();
        assert_eq!(
            names,
            [
                "ggml-tiny.en.bin",
                "text-detection.rten",
                "text-recognition.rten",
                "yamnet.onnx",
                "ggml-small.bin",
                "ggml-medium.bin",
                "ggml-large-v3.bin"
            ]
        );
        let group: Vec<_> = MODELS
            .iter()
            .filter(|m| m.group == Some(MULTILINGUAL))
            .map(|m| m.file)
            .collect();
        assert_eq!(
            group,
            ["ggml-small.bin", "ggml-medium.bin", "ggml-large-v3.bin"]
        );
        assert!(MODELS
            .iter()
            .all(|m| m.sha256.len() == 64 && m.url.starts_with("https://")));
    }
}

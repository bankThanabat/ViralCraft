//! Speaks a script with a real voice engine. `cargo test -p rebuild -- --ignored`.
//! `say` needs macOS; the cloud engines run only when the user's key is set.

use std::fs;
use std::path::Path;

use rebuild::voiceover::{run, Engine, VoiceOptions};

fn cuts3_run() -> tempfile::TempDir {
    let data = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../editspec/tests/data/cuts-3"
    ));
    let dir = tempfile::tempdir().unwrap();
    for f in [
        "probe.json",
        "shots.json",
        "audio.json",
        "inspect.json",
        "judgement.json",
    ] {
        fs::copy(data.join(f), dir.path().join(f)).unwrap();
    }
    dir
}

fn speak(engine: Engine) {
    if let Some(var) = engine.key_var() {
        if std::env::var(var).is_err() {
            eprintln!("skipping: set {var} to test {}", engine.name());
            return;
        }
    }
    let dir = cuts3_run();
    let script = dir.path().join("script.txt");
    fs::write(&script, "One\nTwo\n").unwrap();
    let opts = VoiceOptions {
        engine,
        voice: None,
        model: None,
    };
    let v = run(dir.path(), &script, &opts, |k| std::env::var(k).ok()).unwrap();

    assert_eq!(v.lines.len(), 2);
    assert_eq!((v.lines[0].caption.as_str(), v.lines[0].start), ("c1", 0));
    assert_eq!((v.lines[1].caption.as_str(), v.lines[1].start), ("c2", 60));
    for l in &v.lines {
        assert!(dir.path().join(&l.file).is_file(), "{}", l.file);
        assert!(
            l.seconds > 0.2 && l.seconds < 2.0,
            "{} lasts {}",
            l.text,
            l.seconds
        );
        assert!(l.fits, "one word fits in 2 s");
    }
    assert!(dir.path().join("rebuild/voiceover.json").is_file());
}

#[test]
#[ignore = "needs macOS say and ffmpeg"]
fn say_speaks_one_file_per_caption() {
    if which("say") {
        speak(Engine::Say);
    }
}

#[test]
#[ignore = "needs OPENAI_API_KEY; costs a fraction of a cent"]
fn openai_speaks_one_file_per_caption() {
    speak(Engine::OpenAi);
}

#[test]
#[ignore = "needs ELEVENLABS_API_KEY; uses a few credits"]
fn elevenlabs_speaks_one_file_per_caption() {
    speak(Engine::ElevenLabs);
}

#[test]
#[ignore = "needs MINIMAX_API_KEY; costs a fraction of a cent"]
fn minimax_speaks_one_file_per_caption() {
    speak(Engine::MiniMax);
}

#[test]
#[ignore = "needs DASHSCOPE_API_KEY (Alibaba Model Studio, Singapore region)"]
fn qwen_speaks_one_file_per_caption() {
    speak(Engine::Qwen);
}

fn which(tool: &str) -> bool {
    std::process::Command::new("which")
        .arg(tool)
        .output()
        .is_ok_and(|o| o.status.success())
}

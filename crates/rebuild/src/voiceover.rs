//! `viralcraft voiceover`: an AI voice reads the user's approved script,
//! one line per caption, for clips that have no speech of their own.
//!
//! The user picks the engine: the system's own voices (`say`, free and
//! offline) or a cloud service with their own key. Keys come from the
//! environment and reach curl on stdin, never on its command line.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use model::tools;
use model::{CaptionEvent, VoiceLine, Voiceover};
use serde_json::{json, Value};

use crate::assets::REBUILD_DIR;

pub const VOICEOVER_FILE: &str = "voiceover.json";
const VOICE_DIR: &str = "voiceover";

#[derive(Debug, thiserror::Error)]
pub enum VoiceError {
    #[error("unknown voice engine {0:?}: use say, openai or elevenlabs")]
    Engine(String),
    #[error("{0} is not set: put your own API key in it, or use --engine say")]
    NoKey(&'static str),
    #[error("the script has {lines} lines for {captions} captions: write one line per caption")]
    Count { lines: usize, captions: usize },
    #[error("{engine} failed: {detail}")]
    Speech {
        engine: &'static str,
        detail: String,
    },
    #[error("{path}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error(transparent)]
    Tool(#[from] tools::ToolError),
    #[error(transparent)]
    Run(#[from] editspec::EditSpecError),
}

fn io_err(path: &Path) -> impl FnOnce(std::io::Error) -> VoiceError + '_ {
    move |source| VoiceError::Io {
        path: path.to_path_buf(),
        source,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// macOS `say`: free, offline, a little robotic.
    Say,
    OpenAi,
    ElevenLabs,
}

impl Engine {
    pub fn parse(name: &str) -> Result<Engine, VoiceError> {
        match name.to_lowercase().as_str() {
            "say" => Ok(Engine::Say),
            "openai" => Ok(Engine::OpenAi),
            "elevenlabs" => Ok(Engine::ElevenLabs),
            other => Err(VoiceError::Engine(other.to_string())),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Engine::Say => "say",
            Engine::OpenAi => "openai",
            Engine::ElevenLabs => "elevenlabs",
        }
    }

    /// The environment variable holding the user's key, for cloud engines.
    pub fn key_var(self) -> Option<&'static str> {
        match self {
            Engine::Say => None,
            Engine::OpenAi => Some("OPENAI_API_KEY"),
            Engine::ElevenLabs => Some("ELEVENLABS_API_KEY"),
        }
    }

    /// The model used unless `--model` says otherwise. ElevenLabs'
    /// eleven_multilingual_v2 has no Thai; eleven_v3 does.
    pub fn default_model(self) -> &'static str {
        match self {
            Engine::Say => "",
            Engine::OpenAi => "gpt-4o-mini-tts",
            Engine::ElevenLabs => "eleven_v3",
        }
    }

    /// The voice used unless `--voice` says otherwise. `say` needs a voice
    /// that speaks the script's language.
    pub fn default_voice(self, script: &str) -> &'static str {
        match self {
            Engine::Say if script.chars().any(is_thai) => "Kanya",
            Engine::Say => "Samantha",
            Engine::OpenAi => "marin",
            // The voice in ElevenLabs' API reference example.
            Engine::ElevenLabs => "JBFqnCBsd6RMkjVDRZzb",
        }
    }
}

fn is_thai(c: char) -> bool {
    ('\u{0E00}'..='\u{0E7F}').contains(&c)
}

/// One cloud text-to-speech call.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub url: String,
    /// Full header lines, the key among them.
    pub headers: Vec<String>,
    pub body: Value,
}

/// The call for `text` on a cloud engine; `None` for `say`.
pub fn request(engine: Engine, model: &str, voice: &str, text: &str, key: &str) -> Option<Request> {
    match engine {
        Engine::Say => None,
        Engine::OpenAi => Some(Request {
            url: "https://api.openai.com/v1/audio/speech".to_string(),
            headers: vec![
                format!("Authorization: Bearer {key}"),
                "Content-Type: application/json".to_string(),
            ],
            body: json!({ "model": model, "voice": voice, "input": text, "response_format": "wav" }),
        }),
        Engine::ElevenLabs => Some(Request {
            url: format!(
                "https://api.elevenlabs.io/v1/text-to-speech/{voice}?output_format=mp3_44100_128"
            ),
            headers: vec![
                format!("xi-api-key: {key}"),
                "Content-Type: application/json".to_string(),
            ],
            body: json!({ "text": text, "model_id": model }),
        }),
    }
}

/// curl's config text for `req` (read with `-K -`), so the key travels on
/// stdin and never appears in the process list.
pub fn curl_config(req: &Request, body_file: &Path, out_file: &Path) -> String {
    let quote = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    let mut c = format!("url = {}\n", quote(&req.url));
    for h in &req.headers {
        c.push_str(&format!("header = {}\n", quote(h)));
    }
    c.push_str(&format!(
        "data-binary = {}\n",
        quote(&format!("@{}", body_file.display()))
    ));
    c.push_str(&format!(
        "output = {}\n",
        quote(&out_file.display().to_string())
    ));
    c
}

/// Where each script line goes: under the caption of the same number,
/// with room until the next caption (or the end) starts.
pub fn plan(
    captions: &[CaptionEvent],
    lines: &[String],
    total_frames: i64,
) -> Result<Vec<(String, i64, i64)>, VoiceError> {
    if lines.len() != captions.len() {
        return Err(VoiceError::Count {
            lines: lines.len(),
            captions: captions.len(),
        });
    }
    Ok(captions
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let start = c.range.start.value;
            let next = captions
                .get(i + 1)
                .map_or(total_frames, |n| n.range.start.value);
            (c.id.clone(), start, (next - start).max(0))
        })
        .collect())
}

pub struct VoiceOptions {
    pub engine: Engine,
    pub voice: Option<String>,
    pub model: Option<String>,
}

/// Speaks every line of `script` into `rebuild/voiceover/` and writes
/// `rebuild/voiceover.json`. `key` looks up an environment variable.
pub fn run(
    run_dir: &Path,
    script: &Path,
    opts: &VoiceOptions,
    key: impl Fn(&str) -> Option<String>,
) -> Result<Voiceover, VoiceError> {
    let text = fs::read_to_string(script).map_err(io_err(script))?;
    let lines = crate::prompt::script_lines(&text);
    let inputs = editspec::load(run_dir)?;
    let captions = crate::prompt::rebuilt_captions(&inputs);
    let placed = plan(&captions, &lines, inputs.probe.frames)?;
    let engine = opts.engine;
    let api_key = match engine.key_var() {
        Some(var) => Some(
            key(var)
                .filter(|k| !k.trim().is_empty())
                .ok_or(VoiceError::NoKey(var))?,
        ),
        None => None,
    };
    let model = opts
        .model
        .clone()
        .unwrap_or_else(|| engine.default_model().to_string());
    let voice = opts
        .voice
        .clone()
        .unwrap_or_else(|| engine.default_voice(&text).to_string());

    let dir = run_dir.join(REBUILD_DIR).join(VOICE_DIR);
    fs::create_dir_all(&dir).map_err(io_err(&dir))?;
    let fps = inputs.probe.rate.as_f64();
    let mut out = Vec::new();
    for (i, ((caption, start, room), line)) in placed.into_iter().zip(&lines).enumerate() {
        eprintln!("voiceover: line {} of {}", i + 1, lines.len());
        let raw = dir.join(format!("line-{:02}.raw", i + 1));
        speak(engine, &model, &voice, line, api_key.as_deref(), &raw)?;
        let name = format!("line-{:02}.wav", i + 1);
        let wav = dir.join(&name);
        tools::run(
            "ffmpeg",
            &[
                "-y",
                "-v",
                "error",
                "-i",
                path_str(&raw),
                "-ac",
                "1",
                "-ar",
                "48000",
                path_str(&wav),
            ],
        )?;
        let _ = fs::remove_file(&raw);
        let seconds: f64 = tools::run(
            "ffprobe",
            &[
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "csv=p=0",
                path_str(&wav),
            ],
        )?
        .trim()
        .parse()
        .unwrap_or(0.0);
        out.push(VoiceLine {
            caption,
            text: line.clone(),
            file: Path::new(REBUILD_DIR)
                .join(VOICE_DIR)
                .join(name)
                .to_string_lossy()
                .into_owned(),
            start,
            seconds,
            room,
            fits: seconds * fps <= room as f64,
        });
    }
    let voiceover = Voiceover {
        version: 1,
        engine: engine.name().to_string(),
        voice,
        model,
        lines: out,
    };
    let file = run_dir.join(REBUILD_DIR).join(VOICEOVER_FILE);
    let json = serde_json::to_string_pretty(&voiceover).expect("Voiceover always serializes");
    fs::write(&file, json).map_err(io_err(&file))?;
    Ok(voiceover)
}

fn path_str(p: &Path) -> &str {
    p.to_str().unwrap_or_default()
}

/// One line of speech into `out`, in whatever format the engine returns.
fn speak(
    engine: Engine,
    model: &str,
    voice: &str,
    text: &str,
    key: Option<&str>,
    out: &Path,
) -> Result<(), VoiceError> {
    let Some(req) = request(engine, model, voice, text, key.unwrap_or_default()) else {
        let aiff = out.with_extension("aiff");
        tools::run("say", &["-v", voice, "-o", path_str(&aiff), text])?;
        return fs::rename(&aiff, out).map_err(io_err(out));
    };
    let body = out.with_extension("json");
    fs::write(&body, req.body.to_string()).map_err(io_err(&body))?;
    let mut child = Command::new("curl")
        .args(["-sS", "--fail-with-body", "-K", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(io_err(Path::new("curl")))?;
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(curl_config(&req, &body, out).as_bytes())
        .map_err(io_err(Path::new("curl")))?;
    let done = child
        .wait_with_output()
        .map_err(io_err(Path::new("curl")))?;
    let _ = fs::remove_file(&body);
    if !done.status.success() {
        // With --fail-with-body the service's error message is in `out`.
        let said = fs::read_to_string(out).unwrap_or_default();
        let _ = fs::remove_file(out);
        return Err(VoiceError::Speech {
            engine: engine.name(),
            detail: format!(
                "{} {}",
                String::from_utf8_lossy(&done.stderr).trim(),
                said.chars().take(300).collect::<String>()
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engines_by_name() {
        assert_eq!(Engine::parse("OpenAI").unwrap(), Engine::OpenAi);
        assert_eq!(Engine::parse("say").unwrap(), Engine::Say);
        assert!(matches!(
            Engine::parse("google"),
            Err(VoiceError::Engine(_))
        ));
    }

    #[test]
    fn the_free_voice_speaks_the_scripts_language() {
        assert_eq!(Engine::Say.default_voice("ตักมัทฉะ 4g"), "Kanya");
        assert_eq!(Engine::Say.default_voice("Sift the matcha"), "Samantha");
    }

    #[test]
    fn cloud_engines_need_the_users_own_key() {
        assert_eq!(Engine::OpenAi.key_var(), Some("OPENAI_API_KEY"));
        assert_eq!(Engine::ElevenLabs.key_var(), Some("ELEVENLABS_API_KEY"));
        assert_eq!(Engine::Say.key_var(), None);
    }

    #[test]
    fn openai_request_shape() {
        let r = request(Engine::OpenAi, "gpt-4o-mini-tts", "marin", "สวัสดี", "sk-1").unwrap();
        assert_eq!(r.url, "https://api.openai.com/v1/audio/speech");
        assert!(r
            .headers
            .contains(&"Authorization: Bearer sk-1".to_string()));
        assert_eq!(
            r.body,
            json!({"model":"gpt-4o-mini-tts","voice":"marin","input":"สวัสดี","response_format":"wav"})
        );
    }

    #[test]
    fn elevenlabs_request_shape() {
        let r = request(Engine::ElevenLabs, "eleven_v3", "VOICE", "hi", "xk").unwrap();
        assert_eq!(
            r.url,
            "https://api.elevenlabs.io/v1/text-to-speech/VOICE?output_format=mp3_44100_128"
        );
        assert!(r.headers.contains(&"xi-api-key: xk".to_string()));
        assert_eq!(r.body, json!({"text":"hi","model_id":"eleven_v3"}));
        assert!(request(Engine::Say, "", "Kanya", "hi", "").is_none());
    }

    #[test]
    fn the_key_goes_in_curls_config_quoted() {
        let r = request(Engine::OpenAi, "m", "v", "t", "sk-\"odd\\").unwrap();
        let c = curl_config(&r, Path::new("/tmp/b.json"), Path::new("/tmp/o.raw"));
        assert!(
            c.contains("header = \"Authorization: Bearer sk-\\\"odd\\\\\"\n"),
            "{c}"
        );
        assert!(c.contains("data-binary = \"@/tmp/b.json\"\n"));
        assert!(c.contains("output = \"/tmp/o.raw\"\n"));
    }

    fn caption(id: &str, start: i64) -> CaptionEvent {
        let mut c: CaptionEvent = serde_json::from_value(json!({
            "id": id,
            "range": {"start": {"value": start, "rate": {"num": 30, "den": 1}},
                      "duration": {"value": 30, "rate": {"num": 30, "den": 1}}},
            "text": "x", "lines": ["x"],
            "bounds": {"cx": 0.5, "cy": 0.5, "w": 0.2, "h": 0.05},
            "line_boxes": [], "anchor": "an5", "fill": "#FFFFFF", "outline": null,
            "box_color": null, "line_height": 0.05, "font_px_estimate": 40.0,
            "uppercase": false, "animation": null,
            "safe_zone": {"top": 0.1, "right": 0.1, "bottom": 0.1, "left": 0.1, "inside": true}
        }))
        .unwrap();
        c.id = id.to_string();
        c
    }

    #[test]
    fn each_line_goes_under_its_caption_until_the_next_one() {
        let caps = [caption("c1", 10), caption("c4", 100)];
        let lines = ["one".to_string(), "two".to_string()];
        let got = plan(&caps, &lines, 180).unwrap();
        assert_eq!(
            got,
            [("c1".to_string(), 10, 90), ("c4".to_string(), 100, 80)]
        );
    }

    #[test]
    fn the_script_must_have_one_line_per_caption() {
        let caps = [caption("c1", 10), caption("c4", 100)];
        let err = plan(&caps, &["one".to_string()], 180).unwrap_err();
        assert!(err.to_string().contains("1 lines for 2 captions"), "{err}");
    }

    #[test]
    fn a_missing_key_says_which_variable_to_set() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("s.txt");
        fs::write(&script, "a\n").unwrap();
        let err = run(
            Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../editspec/tests/data/cuts-3"
            )),
            &script,
            &VoiceOptions {
                engine: Engine::OpenAi,
                voice: None,
                model: None,
            },
            |_| None,
        );
        // cuts-3 has two captions to rebuild, so the count fails first;
        // with two lines the missing key is reported.
        assert!(matches!(err, Err(VoiceError::Count { .. })));
        fs::write(&script, "a\nb\n").unwrap();
        let err = run(
            Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../editspec/tests/data/cuts-3"
            )),
            &script,
            &VoiceOptions {
                engine: Engine::OpenAi,
                voice: None,
                model: None,
            },
            |_| None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("OPENAI_API_KEY"), "{err}");
    }
}

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
    #[error("unknown voice engine {0:?}: use say, openai, elevenlabs, minimax, qwen or bytedance")]
    Engine(String),
    #[error(
        "{0} is not set. Run this command in your own terminal: it asks for the key, and you just \
         paste it and press Enter (it stays hidden, nothing shows, and it is used for this run only). \
         Or `export {0}=<key>` there first, or use --engine say"
    )]
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
    /// MiniMax (Hailuo voices), China. Speaks Thai.
    MiniMax,
    /// Alibaba's Qwen-TTS through Model Studio, China. No Thai setting.
    Qwen,
    /// ByteDance's Doubao voices through Volcengine, China. Speaks Thai.
    ByteDance,
}

impl Engine {
    pub fn parse(name: &str) -> Result<Engine, VoiceError> {
        match name.to_lowercase().as_str() {
            "say" => Ok(Engine::Say),
            "openai" => Ok(Engine::OpenAi),
            "elevenlabs" => Ok(Engine::ElevenLabs),
            "minimax" => Ok(Engine::MiniMax),
            "qwen" => Ok(Engine::Qwen),
            "bytedance" | "doubao" | "volcengine" => Ok(Engine::ByteDance),
            other => Err(VoiceError::Engine(other.to_string())),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Engine::Say => "say",
            Engine::OpenAi => "openai",
            Engine::ElevenLabs => "elevenlabs",
            Engine::MiniMax => "minimax",
            Engine::Qwen => "qwen",
            Engine::ByteDance => "bytedance",
        }
    }

    /// The environment variable holding the user's key, for cloud engines.
    pub fn key_var(self) -> Option<&'static str> {
        match self {
            Engine::Say => None,
            Engine::OpenAi => Some("OPENAI_API_KEY"),
            Engine::ElevenLabs => Some("ELEVENLABS_API_KEY"),
            Engine::MiniMax => Some("MINIMAX_API_KEY"),
            Engine::Qwen => Some("DASHSCOPE_API_KEY"),
            Engine::ByteDance => Some("VOLCENGINE_API_KEY"),
        }
    }

    /// The model used unless `--model` says otherwise. ElevenLabs'
    /// eleven_multilingual_v2 has no Thai; eleven_v3 does.
    pub fn default_model(self) -> &'static str {
        match self {
            Engine::Say => "",
            Engine::OpenAi => "gpt-4o-mini-tts",
            Engine::ElevenLabs => "eleven_v3",
            Engine::MiniMax => "speech-2.8-hd",
            Engine::Qwen => "qwen3-tts-flash",
            // The resource id: Doubao speech model 2.0.
            Engine::ByteDance => "seed-tts-2.0",
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
            // Thai: "Aom", from ElevenLabs' library; of its five Thai
            // female voices, the closest to a Thai recipe narrator in
            // pitch (+11%) and pace (-4%) when measured.
            Engine::ElevenLabs if script.chars().any(is_thai) => "nealpwJT5tCyJFKDzEq8",
            Engine::ElevenLabs => "JBFqnCBsd6RMkjVDRZzb",
            // The voices in the providers' API reference examples.
            Engine::MiniMax => "English_expressive_narrator",
            Engine::Qwen => "Cherry",
            Engine::ByteDance => "zh_female_vv_uranus_bigtts",
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
        Engine::MiniMax => Some(Request {
            url: "https://api.minimax.io/v1/t2a_v2".to_string(),
            headers: vec![
                format!("Authorization: Bearer {key}"),
                "Content-Type: application/json".to_string(),
            ],
            body: json!({
                "model": model,
                "text": text,
                "voice_setting": { "voice_id": voice },
                "audio_setting": { "format": "wav", "sample_rate": 44100, "channel": 1 },
                "language_boost": "auto",
                "output_format": "hex"
            }),
        }),
        Engine::Qwen => Some(Request {
            url: "https://dashscope-intl.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation".to_string(),
            headers: vec![
                format!("Authorization: Bearer {key}"),
                "Content-Type: application/json".to_string(),
            ],
            body: json!({
                "model": model,
                "input": { "text": text, "voice": voice, "language_type": "Auto" }
            }),
        }),
        Engine::ByteDance => Some(Request {
            url: "https://openspeech.bytedance.com/api/v3/tts/unidirectional".to_string(),
            headers: vec![
                format!("X-Api-Key: {key}"),
                format!("X-Api-Resource-Id: {model}"),
                format!("X-Api-Request-Id: {}", request_id()),
                "Content-Type: application/json".to_string(),
            ],
            body: json!({ "req_params": {
                "text": text,
                "speaker": voice,
                "audio_params": { "format": "wav", "sample_rate": 24000 },
                // Read the text's own language (Thai among them).
                "additions": "{\"enable_auto_language_recognition\":true}"
            }}),
        }),
    }
}

/// A fresh id in UUID form for each ByteDance call, as its API requires.
fn request_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let bits = now ^ ((u128::from(n) << 64) | u128::from(std::process::id()));
    let h = format!("{bits:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}

/// Standard base64 (with or without padding) to bytes.
fn base64_decode(text: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text
        .bytes()
        .filter(|b| !b.is_ascii_whitespace() && *b != b'=')
    {
        let v = ALPHABET.iter().position(|&a| a == c)? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// What a service's reply holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unwrapped {
    /// The audio file itself.
    Bytes(Vec<u8>),
    /// A link the audio must be fetched from.
    Fetch(String),
}

/// The audio in a successful reply. OpenAI and ElevenLabs return the file;
/// MiniMax wraps it as hex in JSON; Qwen returns a link good for a day;
/// ByteDance streams JSON chunks, each with a base64 piece of the audio.
pub fn unwrap_audio(engine: Engine, reply: &[u8]) -> Result<Unwrapped, VoiceError> {
    let fail = |detail: String| VoiceError::Speech {
        engine: engine.name(),
        detail,
    };
    let parse = || -> Result<Value, VoiceError> {
        serde_json::from_slice(reply).map_err(|e| fail(format!("unreadable reply: {e}")))
    };
    match engine {
        Engine::Say | Engine::OpenAi | Engine::ElevenLabs => Ok(Unwrapped::Bytes(reply.to_vec())),
        Engine::MiniMax => {
            let v = parse()?;
            let code = v["base_resp"]["status_code"].as_i64().unwrap_or(-1);
            if code != 0 {
                return Err(fail(format!(
                    "{} ({code})",
                    v["base_resp"]["status_msg"].as_str().unwrap_or("error")
                )));
            }
            let hex = v["data"]["audio"]
                .as_str()
                .ok_or_else(|| fail("no audio in the reply".to_string()))?;
            let bytes = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(hex.get(i..i + 2).unwrap_or("zz"), 16))
                .collect::<Result<Vec<u8>, _>>()
                .map_err(|_| fail("audio is not hex".to_string()))?;
            Ok(Unwrapped::Bytes(bytes))
        }
        Engine::ByteDance => {
            // One JSON object per chunk, newline-separated or back to back.
            let text = String::from_utf8_lossy(reply);
            let mut audio = Vec::new();
            let mut chunks = 0;
            for v in serde_json::Deserializer::from_str(&text).into_iter::<Value>() {
                let v = v.map_err(|e| fail(format!("unreadable chunk: {e}")))?;
                chunks += 1;
                let code = v["code"].as_i64().unwrap_or(-1);
                // 0 is a chunk; 20000000 is the end marker.
                if code != 0 && code != 20_000_000 {
                    return Err(fail(format!(
                        "{} ({code})",
                        v["message"].as_str().unwrap_or("error")
                    )));
                }
                let piece = v["data"].as_str().unwrap_or_default();
                audio.extend(
                    base64_decode(piece).ok_or_else(|| fail("audio is not base64".to_string()))?,
                );
            }
            if chunks == 0 {
                return Err(fail("empty reply".to_string()));
            }
            if audio.is_empty() {
                return Err(fail("no audio in the reply".to_string()));
            }
            Ok(Unwrapped::Bytes(audio))
        }
        Engine::Qwen => {
            let v = parse()?;
            match v["output"]["audio"]["url"].as_str() {
                Some(url) => Ok(Unwrapped::Fetch(url.to_string())),
                None => Err(fail(format!(
                    "{} ({})",
                    v["message"]
                        .as_str()
                        .unwrap_or("no audio link in the reply"),
                    v["code"].as_str().unwrap_or("?")
                ))),
            }
        }
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
    /// Speak ElevenLabs lines through the user's signed-in `elevenlabs`
    /// tool instead of a key.
    pub elevenlabs_cli: bool,
}

/// Whether `elevenlabs auth status` output says the tool is signed in.
pub fn elevenlabs_logged_in(status_json: &str) -> bool {
    serde_json::from_str::<Value>(status_json).is_ok_and(|v| {
        v["schemes"]
            .as_array()
            .is_some_and(|s| s.iter().any(|x| x["logged_in"] == true))
    })
}

/// Whether ElevenLabs' own CLI is installed and signed in (its login
/// lives in the system keychain, so no key is needed).
pub fn elevenlabs_cli_ready() -> bool {
    Command::new("elevenlabs")
        .args(["auth", "status", "--format", "json"])
        .output()
        .is_ok_and(|o| {
            o.status.success() && elevenlabs_logged_in(&String::from_utf8_lossy(&o.stdout))
        })
}

/// The `elevenlabs` arguments that speak `text` into `out` (MP3). Thai is
/// pinned so short lines are not read in another language.
pub fn elevenlabs_cli_args(voice: &str, model: &str, text: &str, out: &Path) -> Vec<String> {
    let mut args: Vec<String> = [
        "text-to-speech",
        "convert",
        "--voice-id",
        voice,
        "--model-id",
        model,
        "--text",
        text,
        "--output",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    args.push(out.display().to_string());
    if text.chars().any(is_thai) {
        args.extend(["--language-code".to_string(), "th".to_string()]);
    }
    args
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
    let use_cli = engine == Engine::ElevenLabs && opts.elevenlabs_cli;
    let api_key = match engine.key_var() {
        _ if use_cli => None,
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
        if use_cli {
            let mp3 = raw.with_extension("mp3");
            tools::run(
                "elevenlabs",
                &elevenlabs_cli_args(&voice, &model, line, &mp3)
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
            )?;
            fs::rename(&mp3, &raw).map_err(io_err(&raw))?;
        } else {
            speak(engine, &model, &voice, line, api_key.as_deref(), &raw)?;
        }
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
    let reply = fs::read(out).map_err(io_err(out))?;
    match unwrap_audio(engine, &reply)? {
        Unwrapped::Bytes(bytes) => fs::write(out, bytes).map_err(io_err(out)),
        Unwrapped::Fetch(url) => {
            // The link needs no key.
            tools::run("curl", &["-sS", "--fail", "-o", path_str(out), &url])?;
            Ok(())
        }
    }
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
    fn minimax_request_shape() {
        let r = request(
            Engine::MiniMax,
            "speech-2.8-hd",
            "English_expressive_narrator",
            "สวัสดี",
            "mk",
        )
        .unwrap();
        assert_eq!(r.url, "https://api.minimax.io/v1/t2a_v2");
        assert!(r.headers.contains(&"Authorization: Bearer mk".to_string()));
        assert_eq!(
            r.body,
            json!({
                "model": "speech-2.8-hd",
                "text": "สวัสดี",
                "voice_setting": {"voice_id": "English_expressive_narrator"},
                "audio_setting": {"format": "wav", "sample_rate": 44100, "channel": 1},
                "language_boost": "auto",
                "output_format": "hex"
            })
        );
    }

    #[test]
    fn qwen_request_shape() {
        let r = request(Engine::Qwen, "qwen3-tts-flash", "Cherry", "hello", "ds").unwrap();
        assert_eq!(
            r.url,
            "https://dashscope-intl.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation"
        );
        assert!(r.headers.contains(&"Authorization: Bearer ds".to_string()));
        assert_eq!(
            r.body,
            json!({"model": "qwen3-tts-flash", "input": {"text": "hello", "voice": "Cherry", "language_type": "Auto"}})
        );
    }

    #[test]
    fn chinese_engines_need_their_own_keys_and_models() {
        assert_eq!(Engine::parse("MiniMax").unwrap(), Engine::MiniMax);
        assert_eq!(Engine::parse("qwen").unwrap(), Engine::Qwen);
        assert_eq!(Engine::MiniMax.key_var(), Some("MINIMAX_API_KEY"));
        assert_eq!(Engine::Qwen.key_var(), Some("DASHSCOPE_API_KEY"));
        assert_eq!(Engine::MiniMax.default_model(), "speech-2.8-hd");
        assert_eq!(Engine::Qwen.default_model(), "qwen3-tts-flash");
    }

    #[test]
    fn minimax_audio_is_hex_inside_json() {
        // "RIFF" as hex, the start of any WAV file.
        let body = json!({"data": {"audio": "52494646", "status": 2}, "base_resp": {"status_code": 0, "status_msg": "success"}});
        assert_eq!(
            unwrap_audio(Engine::MiniMax, body.to_string().as_bytes()).unwrap(),
            Unwrapped::Bytes(b"RIFF".to_vec())
        );
        let bad = json!({"base_resp": {"status_code": 1004, "status_msg": "invalid api key"}});
        let err = unwrap_audio(Engine::MiniMax, bad.to_string().as_bytes()).unwrap_err();
        assert!(err.to_string().contains("invalid api key"), "{err}");
    }

    #[test]
    fn qwen_audio_is_a_link_to_fetch() {
        let body = json!({"output": {"finish_reason": "stop", "audio": {"url": "https://x.example/a.wav", "data": ""}}});
        assert_eq!(
            unwrap_audio(Engine::Qwen, body.to_string().as_bytes()).unwrap(),
            Unwrapped::Fetch("https://x.example/a.wav".to_string())
        );
        let bad = json!({"code": "InvalidApiKey", "message": "Invalid API-key provided."});
        let err = unwrap_audio(Engine::Qwen, bad.to_string().as_bytes()).unwrap_err();
        assert!(err.to_string().contains("Invalid API-key"), "{err}");
    }

    #[test]
    fn file_engines_pass_the_bytes_through() {
        assert_eq!(
            unwrap_audio(Engine::OpenAi, b"RIFF").unwrap(),
            Unwrapped::Bytes(b"RIFF".to_vec())
        );
        assert_eq!(
            unwrap_audio(Engine::ElevenLabs, b"ID3").unwrap(),
            Unwrapped::Bytes(b"ID3".to_vec())
        );
    }

    #[test]
    fn bytedance_request_shape() {
        let r = request(
            Engine::ByteDance,
            "seed-tts-2.0",
            "zh_female_vv_uranus_bigtts",
            "สวัสดี",
            "bk",
        )
        .unwrap();
        assert_eq!(
            r.url,
            "https://openspeech.bytedance.com/api/v3/tts/unidirectional"
        );
        assert!(r.headers.contains(&"X-Api-Key: bk".to_string()));
        assert!(r
            .headers
            .contains(&"X-Api-Resource-Id: seed-tts-2.0".to_string()));
        let request_id = r
            .headers
            .iter()
            .find_map(|h| h.strip_prefix("X-Api-Request-Id: "))
            .expect("a request id");
        assert_eq!(request_id.len(), 36, "a uuid: {request_id}");
        assert_eq!(
            r.body,
            json!({"req_params": {
                "text": "สวัสดี",
                "speaker": "zh_female_vv_uranus_bigtts",
                "audio_params": {"format": "wav", "sample_rate": 24000},
                "additions": "{\"enable_auto_language_recognition\":true}"
            }})
        );
        // Each call gets its own id.
        let again = request(Engine::ByteDance, "seed-tts-2.0", "v", "t", "bk").unwrap();
        assert_ne!(again.headers, r.headers);
    }

    #[test]
    fn bytedance_audio_arrives_as_base64_chunks() {
        // Two chunks, newline-separated, then the end marker; "RIFF" + "WAVE".
        let reply = concat!(
            "{\"code\":0,\"message\":\"OK\",\"data\":\"UklGRg==\",\"sentence\":{}}\n",
            "{\"code\":0,\"message\":\"OK\",\"data\":\"V0FWRQ==\"}\n",
            "{\"code\":20000000,\"message\":\"OK\",\"data\":\"\",\"usage\":{\"text_words\":4}}\n"
        );
        assert_eq!(
            unwrap_audio(Engine::ByteDance, reply.as_bytes()).unwrap(),
            Unwrapped::Bytes(b"RIFFWAVE".to_vec())
        );
        // Chunks back to back without newlines are read the same way.
        let packed = "{\"code\":0,\"data\":\"UklGRg==\"}{\"code\":0,\"data\":\"V0FWRQ==\"}";
        assert_eq!(
            unwrap_audio(Engine::ByteDance, packed.as_bytes()).unwrap(),
            Unwrapped::Bytes(b"RIFFWAVE".to_vec())
        );
        let bad = "{\"code\":45000001,\"message\":\"invalid speaker\",\"data\":\"\"}";
        let err = unwrap_audio(Engine::ByteDance, bad.as_bytes()).unwrap_err();
        assert!(err.to_string().contains("invalid speaker"), "{err}");
        let empty = "{\"code\":0,\"data\":\"\"}";
        assert!(
            unwrap_audio(Engine::ByteDance, empty.as_bytes()).is_err(),
            "no audio is an error"
        );
    }

    #[test]
    fn bytedance_is_an_engine_with_its_own_key() {
        assert_eq!(Engine::parse("ByteDance").unwrap(), Engine::ByteDance);
        assert_eq!(
            Engine::parse("doubao").unwrap(),
            Engine::ByteDance,
            "the voices' brand name works too"
        );
        assert_eq!(Engine::ByteDance.key_var(), Some("VOLCENGINE_API_KEY"));
        assert_eq!(Engine::ByteDance.default_model(), "seed-tts-2.0");
    }

    #[test]
    fn base64_decodes_the_standard_alphabet() {
        assert_eq!(base64_decode("aGVsbG8gd29ybGQ=").unwrap(), b"hello world");
        assert_eq!(
            base64_decode("aGVsbG8gd29ybGQ").unwrap(),
            b"hello world",
            "padding optional"
        );
        assert_eq!(base64_decode("+/8=").unwrap(), [0xFB, 0xFF]);
        assert_eq!(base64_decode("").unwrap(), b"");
        assert!(base64_decode("not base64!").is_none());
    }

    #[test]
    fn the_elevenlabs_tool_is_used_when_signed_in() {
        let status = r#"{"backend":"macOS Keychain","schemes":[{"cli":"elevenlabs","logged_in":true,"scheme":"OAuth"}]}"#;
        assert!(elevenlabs_logged_in(status));
        let out = r#"{"schemes":[{"cli":"elevenlabs","logged_in":false}]}"#;
        assert!(!elevenlabs_logged_in(out));
        assert!(!elevenlabs_logged_in("not json"));
    }

    #[test]
    fn the_elevenlabs_tool_gets_the_voice_model_text_and_language() {
        let args = elevenlabs_cli_args("VID", "eleven_v3", "ตักมัทฉะ", Path::new("/tmp/l.mp3"));
        assert_eq!(
            args,
            [
                "text-to-speech",
                "convert",
                "--voice-id",
                "VID",
                "--model-id",
                "eleven_v3",
                "--text",
                "ตักมัทฉะ",
                "--output",
                "/tmp/l.mp3",
                "--language-code",
                "th",
            ]
        );
        let en = elevenlabs_cli_args("VID", "eleven_v3", "Hello", Path::new("/tmp/l.mp3"));
        assert!(
            !en.contains(&"--language-code".to_string()),
            "only Thai is pinned"
        );
    }

    #[test]
    fn a_thai_script_gets_a_thai_elevenlabs_voice() {
        assert_eq!(
            Engine::ElevenLabs.default_voice("ตักมัทฉะ 4g"),
            "nealpwJT5tCyJFKDzEq8"
        );
        assert_eq!(
            Engine::ElevenLabs.default_voice("Sift the matcha"),
            "JBFqnCBsd6RMkjVDRZzb"
        );
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
                elevenlabs_cli: false,
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
                elevenlabs_cli: false,
            },
            |_| None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("OPENAI_API_KEY"), "{err}");
    }
}

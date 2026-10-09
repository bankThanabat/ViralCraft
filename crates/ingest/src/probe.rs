//! Parses `ffprobe -show_streams -show_format -of json` output.

use model::{AudioInfo, Fps, RationalTime};
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("ffprobe output is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the file has no video stream")]
    NoVideoStream,
    #[error("ffprobe reported no usable {0}")]
    Missing(&'static str),
    #[error(transparent)]
    Rate(#[from] model::TimeError),
}

/// What ffprobe says about the first video stream and first audio stream.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    /// The constant rate to normalize to.
    pub rate: Fps,
    /// True when the real and average frame rates differ.
    pub vfr: bool,
    pub width: u32,
    pub height: u32,
    pub aspect: String,
    pub frames: i64,
    pub audio: Option<AudioInfo>,
}

/// Rates a VFR file gets snapped to; phones record at one of these nominally.
const STANDARD_RATES: [(u32, u32); 8] = [
    (24000, 1001),
    (24, 1),
    (25, 1),
    (30000, 1001),
    (30, 1),
    (50, 1),
    (60000, 1001),
    (60, 1),
];

pub fn parse_ffprobe(json: &str) -> Result<Parsed, ProbeError> {
    let root: Value = serde_json::from_str(json)?;
    let streams = root["streams"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let video = streams
        .iter()
        .find(|s| s["codec_type"] == "video")
        .ok_or(ProbeError::NoVideoStream)?;

    let real: Fps = str_field(video, "r_frame_rate")?.parse()?;
    let average: Fps = str_field(video, "avg_frame_rate")?.parse()?;
    let vfr = real != average;
    let rate = if vfr { snap(average) } else { real };

    let width = num_field(video, "width")? as u32;
    let height = num_field(video, "height")? as u32;

    let frames = match video["nb_frames"].as_str().and_then(|n| n.parse().ok()) {
        Some(n) if !vfr => n,
        _ => {
            let duration = seconds(video, "duration")
                .or_else(|| seconds(&root["format"], "duration"))
                .ok_or(ProbeError::Missing("duration"))?;
            RationalTime::from_seconds(duration, rate).value
        }
    };

    let video_start = seconds(video, "start_time").unwrap_or(0.0);
    let audio = streams
        .iter()
        .find(|s| s["codec_type"] == "audio")
        .map(|a| -> Result<AudioInfo, ProbeError> {
            Ok(AudioInfo {
                sample_rate: str_field(a, "sample_rate")?
                    .parse()
                    .map_err(|_| ProbeError::Missing("audio sample_rate"))?,
                channels: num_field(a, "channels")? as u32,
                offset: RationalTime::from_seconds(
                    seconds(a, "start_time").unwrap_or(0.0) - video_start,
                    rate,
                ),
            })
        })
        .transpose()?;

    Ok(Parsed {
        rate,
        vfr,
        width,
        height,
        aspect: aspect(width, height),
        frames,
        audio,
    })
}

fn snap(average: Fps) -> Fps {
    snap_rate(average.as_f64())
}

/// The standard rate nearest to `fps`.
pub fn snap_rate(fps: f64) -> Fps {
    let target = fps;
    let (num, den) = STANDARD_RATES
        .into_iter()
        .min_by(|a, b| {
            let da = (f64::from(a.0) / f64::from(a.1) - target).abs();
            let db = (f64::from(b.0) / f64::from(b.1) - target).abs();
            da.total_cmp(&db)
        })
        .expect("STANDARD_RATES is not empty");
    Fps { num, den }
}

fn str_field<'a>(v: &'a Value, key: &'static str) -> Result<&'a str, ProbeError> {
    v[key].as_str().ok_or(ProbeError::Missing(key))
}

fn num_field(v: &Value, key: &'static str) -> Result<u64, ProbeError> {
    v[key].as_u64().ok_or(ProbeError::Missing(key))
}

fn seconds(v: &Value, key: &str) -> Option<f64> {
    v[key].as_str()?.parse().ok()
}

/// Frame spacing measured from packet timestamps.
#[derive(Debug, Clone, PartialEq)]
pub struct PacketTiming {
    pub median_interval: f64,
    intervals: Vec<f64>,
}

impl PacketTiming {
    /// True when any frame is spaced more than a quarter frame off `rate`.
    /// ffprobe's rate fields miss this: a jittered file can report the same
    /// real and average rate.
    pub fn irregular(&self, rate: Fps) -> bool {
        let expected = 1.0 / rate.as_f64();
        self.intervals
            .iter()
            .any(|i| (i - expected).abs() > expected * 0.25)
    }
}

/// Parses `ffprobe -select_streams v:0 -show_entries packet=pts_time -of
/// csv=p=0` output. Packets arrive in decode order, so they are sorted first.
pub fn packet_timing(pts_csv: &str) -> Option<PacketTiming> {
    let mut pts: Vec<f64> = pts_csv
        .lines()
        .filter_map(|l| l.trim().trim_end_matches(',').parse().ok())
        .collect();
    if pts.len() < 2 {
        return None;
    }
    pts.sort_by(f64::total_cmp);
    let intervals: Vec<f64> = pts.windows(2).map(|w| w[1] - w[0]).collect();
    let mut sorted = intervals.clone();
    sorted.sort_by(f64::total_cmp);
    let median_interval = sorted[sorted.len() / 2];
    Some(PacketTiming {
        median_interval,
        intervals,
    })
}

/// `1080x1920` → `9:16`.
pub fn aspect(width: u32, height: u32) -> String {
    let g = gcd(width, height).max(1);
    format!("{}:{}", width / g, height / g)
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::Fps;

    const NTSC: Fps = Fps {
        num: 30000,
        den: 1001,
    };

    // Trimmed from real ffprobe 9.0 output for a 1080x1920 H.264 + AAC file.
    const PHONE: &str = r#"{
      "streams": [
        { "index": 0, "codec_type": "video", "codec_name": "h264",
          "width": 1080, "height": 1920,
          "r_frame_rate": "30000/1001", "avg_frame_rate": "30000/1001",
          "start_time": "0.000000", "duration": "1.968633", "nb_frames": "59" },
        { "index": 1, "codec_type": "audio", "codec_name": "aac",
          "sample_rate": "44100", "channels": 1,
          "r_frame_rate": "0/0", "avg_frame_rate": "0/0",
          "start_time": "0.066733", "duration": "2.000000", "nb_frames": "88" }
      ],
      "format": { "format_name": "mov,mp4,m4a,3gp,3g2,mj2", "duration": "2.000000" }
    }"#;

    #[test]
    fn parses_rate_size_aspect_and_frames() {
        let p = parse_ffprobe(PHONE).unwrap();
        assert_eq!(p.rate, NTSC);
        assert_eq!((p.width, p.height), (1080, 1920));
        assert_eq!(p.aspect, "9:16");
        assert_eq!(p.frames, 59);
        assert!(!p.vfr);
    }

    #[test]
    fn audio_offset_is_relative_to_video_start_in_frames() {
        let p = parse_ffprobe(PHONE).unwrap();
        let audio = p.audio.unwrap();
        assert_eq!(audio.sample_rate, 44100);
        assert_eq!(audio.channels, 1);
        // 0.066733 s at 29.97 fps is 2 frames.
        assert_eq!(audio.offset.value, 2);
        assert_eq!(audio.offset.rate, NTSC);
    }

    #[test]
    fn differing_real_and_average_rates_mean_vfr_snapped_to_a_standard_rate() {
        let vfr = PHONE.replace(
            r#""r_frame_rate": "30000/1001", "avg_frame_rate": "30000/1001""#,
            r#""r_frame_rate": "90000/1", "avg_frame_rate": "2997/101""#,
        );
        let p = parse_ffprobe(&vfr).unwrap();
        assert!(p.vfr);
        assert_eq!(p.rate, NTSC, "29.67 fps average snaps to 30000/1001");
    }

    #[test]
    fn missing_nb_frames_falls_back_to_duration_times_rate() {
        let no_count = PHONE.replace(r#", "nb_frames": "59""#, "");
        assert_eq!(parse_ffprobe(&no_count).unwrap().frames, 59);
    }

    #[test]
    fn audio_only_input_is_rejected() {
        let audio_only = r#"{"streams":[{"codec_type":"audio","sample_rate":"16000","channels":1}],"format":{}}"#;
        assert!(matches!(
            parse_ffprobe(audio_only),
            Err(ProbeError::NoVideoStream)
        ));
    }

    #[test]
    fn malformed_json_is_an_error_not_a_panic() {
        assert!(matches!(parse_ffprobe("{"), Err(ProbeError::Json(_))));
    }

    #[test]
    fn evenly_spaced_packets_in_decode_order_are_regular() {
        // Real ffprobe packet pts for a 29.97 fps H.264 file with B-frames.
        let pts =
            "0.000000\n0.100100\n0.033367\n0.066733\n0.233567\n0.166833\n0.133467\n0.200200\n";
        let timing = packet_timing(pts).unwrap();
        assert!(!timing.irregular(NTSC));
        assert_eq!(snap_rate(1.0 / timing.median_interval), NTSC);
    }

    #[test]
    fn jittered_packets_are_irregular_even_when_ffprobe_rates_agree() {
        // Real pts from a file ffprobe labels 30/1 for both rates.
        let pts =
            "0.000000\n0.066667\n0.033333\n0.100000\n0.100000\n0.233333\n0.133333\n0.166667\n";
        let timing = packet_timing(pts).unwrap();
        assert!(timing.irregular(Fps::new(30, 1).unwrap()));
        assert_eq!(
            snap_rate(1.0 / timing.median_interval),
            Fps::new(30, 1).unwrap()
        );
    }

    #[test]
    fn fewer_than_two_packets_gives_no_timing() {
        assert!(packet_timing("0.0\n").is_none());
        assert!(packet_timing("garbage\n").is_none());
    }

    #[test]
    fn aspect_reduces_by_gcd() {
        assert_eq!(aspect(1920, 1080), "16:9");
        assert_eq!(aspect(1080, 1350), "4:5");
        assert_eq!(aspect(720, 720), "1:1");
    }
}

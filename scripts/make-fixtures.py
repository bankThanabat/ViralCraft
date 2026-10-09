#!/usr/bin/env python3
"""Generate the test fixtures in tests/fixtures/ and their expected values.

Run from the repo root:  python3 -I scripts/make-fixtures.py

Needs Pillow and ffmpeg. Everything except speech is byte-identical on
every run. speech.wav is only rebuilt with --speech (needs macOS `say`),
because `say` output drifts by a few milliseconds between runs; the
committed speech.wav and speech.json are always generated together. Frames are rendered with Pillow because Homebrew's
ffmpeg has no drawtext filter; ffmpeg only assembles and encodes them.

Every expected value the Rust tests check lives in fixtures.json, written
from the same constants used to draw the frames.
"""

import array
import json
import math
import os
import random
import shutil
import subprocess
import sys
import tempfile
import wave

from PIL import Image, ImageDraw, ImageFont

OUT = os.path.join("tests", "fixtures")
FPS = 30

BOLD_FONTS = [
    "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
]

# cuts-3: three 2 s scenes, hard cuts at f60 and f120, one caption each.
CUTS_SIZE = (1080, 1920)
CUTS_SCENES = [
    {"color": "#1E2A38", "text": "ONE", "center": (0.50, 0.18)},
    {"color": "#C9A96E", "text": "TWO", "center": (0.50, 0.50)},
    {"color": "#2E7D32", "text": "THREE", "center": (0.50, 0.80)},
]
SCENE_FRAMES = 60
CAPTION_PX = 96
CAPTION_STROKE = 3
BPM = 120

# zoom-1: one 3 s scene scaling 100% -> 120% with CSS ease-out.
ZOOM_SIZE = (540, 960)
ZOOM_FRAMES = 90
ZOOM_FROM, ZOOM_TO = 1.00, 1.20
EASE_OUT = (0.0, 0.0, 0.58, 1.0)  # cubic-bezier(x1, y1, x2, y2)

# speech: words synthesized one at a time and placed at known offsets.
WORDS = "Stop scrolling this one trick will change how you edit video".split()
WORD_GAP = 0.12
SPEECH_RATE = 16000


def font(size):
    for path in BOLD_FONTS:
        if os.path.exists(path):
            return ImageFont.truetype(path, size)
    sys.exit("no bold TrueType font found; add one to BOLD_FONTS")


def hex_rgb(h):
    return tuple(int(h[i : i + 2], 16) for i in (1, 3, 5))


def ffmpeg(*args):
    subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", *args], check=True)


def draw_caption(img, text, center, px, stroke):
    w, h = img.size
    d = ImageDraw.Draw(img)
    pos = (center[0] * w, center[1] * h)
    d.text(pos, text, font=font(px), fill="white", anchor="mm",
           stroke_width=stroke, stroke_fill="black")
    left, top, right, bottom = d.textbbox(pos, text, font=font(px), anchor="mm",
                                          stroke_width=stroke)
    return {
        "center": [round((left + right) / 2 / w, 4), round((top + bottom) / 2 / h, 4)],
        "box": [round((right - left) / w, 4), round((bottom - top) / h, 4)],
    }


def click_track(seconds, bpm, rate=48000):
    """A 1 kHz, 10 ms decaying click on every beat."""
    samples = array.array("h", [0] * int(seconds * rate))
    beats = []
    t = 0.0
    while t < seconds - 1e-9:
        start = int(round(t * rate))
        for i in range(int(0.010 * rate)):
            if start + i < len(samples):
                env = math.exp(-i / (0.002 * rate))
                samples[start + i] = int(20000 * env * math.sin(2 * math.pi * 1000 * i / rate))
        beats.append(round(t, 3))
        t += 60.0 / bpm
    return samples, beats, rate


def write_wav(path, samples, rate):
    with wave.open(path, "wb") as f:
        f.setnchannels(1)
        f.setsampwidth(2)
        f.setframerate(rate)
        f.writeframes(samples.tobytes())


def make_cuts(tmp):
    frames_dir = os.path.join(tmp, "cuts")
    os.makedirs(frames_dir)
    captions = []
    for i, scene in enumerate(CUTS_SCENES):
        img = Image.new("RGB", CUTS_SIZE, hex_rgb(scene["color"]))
        geom = draw_caption(img, scene["text"], scene["center"], CAPTION_PX, CAPTION_STROKE)
        img.save(os.path.join(frames_dir, f"scene{i}.png"))
        captions.append({
            "text": scene["text"],
            "start_frame": i * SCENE_FRAMES,
            "end_frame": (i + 1) * SCENE_FRAMES,
            "fill": "#FFFFFF",
            "outline": "#000000",
            "size_px": CAPTION_PX,
            **geom,
        })
    # Keep scene0.png as the reference frame other fixtures may reuse.
    shutil.copy(os.path.join(frames_dir, "scene0.png"), os.path.join(OUT, "cuts-3-scene0.png"))

    seconds = len(CUTS_SCENES) * SCENE_FRAMES / FPS
    samples, beats, rate = click_track(seconds, BPM)
    wav = os.path.join(tmp, "clicks.wav")
    write_wav(wav, samples, rate)

    concat = os.path.join(tmp, "cuts.txt")
    with open(concat, "w") as f:
        for i in range(len(CUTS_SCENES)):
            f.write(f"file '{os.path.join(frames_dir, f'scene{i}.png')}'\n")
            f.write(f"duration {SCENE_FRAMES / FPS}\n")
        f.write(f"file '{os.path.join(frames_dir, f'scene{len(CUTS_SCENES) - 1}.png')}'\n")

    out = os.path.join(OUT, "cuts-3.mp4")
    ffmpeg("-f", "concat", "-safe", "0", "-i", concat, "-i", wav,
           "-vf", f"fps={FPS},format=yuv420p", "-fps_mode", "cfr",
           "-frames:v", str(len(CUTS_SCENES) * SCENE_FRAMES),
           "-c:v", "libx264", "-preset", "slow", "-crf", "30", "-tune", "stillimage",
           "-c:a", "aac", "-b:a", "48k", "-shortest", "-movflags", "+faststart", out)

    # Same pictures with jittered timestamps. The jitter stays monotonic
    # (|0.012 * d(sin)| < one frame) and exceeds a quarter frame, so ingest
    # must flag it as VFR and restore the cut frames.
    # A fine encoder time base keeps the jitter; the default 1/30 snaps it away.
    ffmpeg("-i", out, "-vf", "setpts='(N/30+0.012*sin(N))/TB'", "-fps_mode", "passthrough",
           "-enc_time_base", "1:90000", "-video_track_timescale", "90000",
           "-c:v", "libx264", "-preset", "slow", "-crf", "30", "-tune", "stillimage",
           "-c:a", "copy", "-movflags", "+faststart", os.path.join(OUT, "cuts-3-vfr.mp4"))

    return {
        "file": "cuts-3.mp4",
        "vfr_variant": "cuts-3-vfr.mp4",
        "fps": FPS,
        "size": list(CUTS_SIZE),
        "frames": len(CUTS_SCENES) * SCENE_FRAMES,
        "cut_frames": [SCENE_FRAMES * i for i in range(1, len(CUTS_SCENES))],
        "scene_colors": [s["color"] for s in CUTS_SCENES],
        "captions": captions,
        "bpm": BPM,
        "beats": beats,
    }


def bezier_ease(x, x1, y1, x2, y2):
    """y for x on cubic-bezier(x1, y1, x2, y2), solved by bisection."""
    def coord(t, a, b):
        return 3 * a * t * (1 - t) ** 2 + 3 * b * t ** 2 * (1 - t) + t ** 3

    lo, hi = 0.0, 1.0
    for _ in range(60):
        mid = (lo + hi) / 2
        if coord(mid, x1, x2) < x:
            lo = mid
        else:
            hi = mid
    return coord((lo + hi) / 2, y1, y2)


def textured(size, seed=7):
    """A busy but compressible background: seeded shapes, no pixel noise."""
    rng = random.Random(seed)
    w, h = size
    img = Image.new("RGB", size, (40, 44, 52))
    d = ImageDraw.Draw(img)
    for _ in range(260):
        x, y = rng.randrange(w), rng.randrange(h)
        r = rng.randrange(8, 60)
        color = tuple(rng.randrange(60, 255) for _ in range(3))
        if rng.random() < 0.5:
            d.ellipse((x - r, y - r, x + r, y + r), fill=color)
        else:
            d.rectangle((x - r, y - r // 2, x + r, y + r // 2), fill=color)
    return img


def make_zoom(tmp):
    frames_dir = os.path.join(tmp, "zoom")
    os.makedirs(frames_dir)
    w, h = ZOOM_SIZE
    # Render the source at 2x so cropping never upsamples past the original.
    base = textured((w * 2, h * 2))
    scales = []
    for f in range(ZOOM_FRAMES):
        progress = f / (ZOOM_FRAMES - 1)
        s = ZOOM_FROM + (ZOOM_TO - ZOOM_FROM) * bezier_ease(progress, *EASE_OUT)
        scales.append(round(s * 100, 3))
        cw, ch = base.width / s, base.height / s
        left, top = (base.width - cw) / 2, (base.height - ch) / 2
        frame = base.resize(ZOOM_SIZE, Image.LANCZOS, box=(left, top, left + cw, top + ch))
        frame.save(os.path.join(frames_dir, f"f{f:04d}.png"))

    ffmpeg("-framerate", str(FPS), "-i", os.path.join(frames_dir, "f%04d.png"),
           "-vf", "format=yuv420p", "-c:v", "libx264", "-preset", "slow", "-crf", "28",
           "-movflags", "+faststart", os.path.join(OUT, "zoom-1.mp4"))
    return {
        "file": "zoom-1.mp4",
        "fps": FPS,
        "size": list(ZOOM_SIZE),
        "frames": ZOOM_FRAMES,
        "anchor": [0.5, 0.5],
        "scale_from": ZOOM_FROM * 100,
        "scale_to": ZOOM_TO * 100,
        "easing": {"o": {"x": EASE_OUT[0], "y": EASE_OUT[1]}, "i": {"x": EASE_OUT[2], "y": EASE_OUT[3]}},
        "scale_per_frame": scales,
    }


def trimmed_word(tmp, word):
    """One spoken word as 16 kHz mono samples with silence trimmed."""
    aiff = os.path.join(tmp, f"{word}.aiff")
    wav = os.path.join(tmp, f"{word}.wav")
    subprocess.run(["say", "-o", aiff, word], check=True)
    ffmpeg("-i", aiff, "-ar", str(SPEECH_RATE), "-ac", "1", "-c:a", "pcm_s16le", wav)
    with wave.open(wav) as f:
        samples = array.array("h", f.readframes(f.getnframes()))
    loud = [i for i, v in enumerate(samples) if abs(v) > 500]
    return samples[loud[0] : loud[-1] + 1]


def make_speech(tmp):
    path = os.path.join(OUT, "speech.wav")
    meta_path = os.path.join(OUT, "speech.json")
    if "--speech" not in sys.argv and os.path.exists(meta_path):
        with open(meta_path) as f:
            return json.load(f)
    if shutil.which("say") is None:
        sys.exit("speech.wav needs macOS `say`; rerun on a Mac with --speech")

    lead = int(0.3 * SPEECH_RATE)
    samples = array.array("h", [0] * lead)
    words = []
    for word in WORDS:
        clip = trimmed_word(tmp, word)
        words.append({
            "text": word,
            "start": round(len(samples) / SPEECH_RATE, 3),
            "end": round((len(samples) + len(clip)) / SPEECH_RATE, 3),
        })
        samples.extend(clip)
        samples.extend([0] * int(WORD_GAP * SPEECH_RATE))
    samples.extend([0] * lead)
    write_wav(path, samples, SPEECH_RATE)
    meta = {"file": "speech.wav", "rate": SPEECH_RATE, "words": words}
    with open(meta_path, "w") as f:
        json.dump(meta, f, indent=2)
    return meta


# A Thai sentence for the multilingual speech check, spoken by macOS's
# Thai voice. Rebuilt only with --speech, like speech.wav.
THAI_TEXT = "สวัสดีค่ะ วันนี้เราจะมาทำมัทฉะลาเต้กัน"


def make_thai_speech(tmp):
    path = os.path.join(OUT, "speech-th.wav")
    meta_path = os.path.join(OUT, "speech-th.json")
    if "--speech" not in sys.argv and os.path.exists(meta_path):
        with open(meta_path) as f:
            return json.load(f)
    if shutil.which("say") is None:
        sys.exit("speech-th.wav needs macOS `say` with the Kanya voice")
    aiff = os.path.join(tmp, "th.aiff")
    subprocess.run(["say", "-v", "Kanya", "-o", aiff, THAI_TEXT], check=True)
    ffmpeg("-i", aiff, "-ar", str(SPEECH_RATE), "-ac", "1", "-c:a", "pcm_s16le", path)
    meta = {"file": "speech-th.wav", "language": "th", "text": THAI_TEXT}
    with open(meta_path, "w") as f:
        json.dump(meta, f, ensure_ascii=False, indent=2)
    return meta


def main():
    os.makedirs(OUT, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        fixtures = {
            "cuts": make_cuts(tmp),
            "zoom": make_zoom(tmp),
            "speech": make_speech(tmp),
            "speech_th": make_thai_speech(tmp),
        }
    with open(os.path.join(OUT, "fixtures.json"), "w") as f:
        json.dump(fixtures, f, indent=2)
        f.write("\n")
    for name in sorted(os.listdir(OUT)):
        print(f"{os.path.getsize(os.path.join(OUT, name)):>9}  {name}")


if __name__ == "__main__":
    main()

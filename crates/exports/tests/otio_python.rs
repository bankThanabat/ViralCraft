//! Round-trips the timeline through the reference OpenTimelineIO library.
//! Needs Python with `opentimelineio`; set `OTIO_PYTHON` to its
//! interpreter (default `python3`). Skips with a message when it is absent.

use std::path::Path;
use std::process::Command;

const CHECK: &str = r#"
import sys, json
import opentimelineio as otio
path, frames = sys.argv[1], float(sys.argv[2])
first = otio.adapters.read_from_file(path)
again = otio.adapters.read_from_string(otio.adapters.write_to_string(first, "otio_json"), "otio_json")
def summary(tl):
    out = []
    for t in tl.tracks:
        items = []
        for c in t:
            if isinstance(c, otio.schema.Transition):
                items.append(("T", c.transition_type, c.in_offset.value, c.out_offset.value))
            else:
                r = c.source_range
                items.append((c.schema_name(), c.name, r.start_time.value, r.duration.value))
        marks = [(m.name, m.marked_range.start_time.value) for m in t.markers]
        out.append((t.name, t.kind, t.duration().value, items, marks))
    return out
a, b = summary(first), summary(again)
assert a == b, "round trip changed the timeline"
for name, kind, duration, _, _ in a:
    assert duration == frames, f"{name} lasts {duration}, expected {frames}"
print(json.dumps({"tracks": len(a), "clips": sum(len(t[3]) for t in a), "markers": sum(len(t[4]) for t in a)}))
"#;

#[test]
#[ignore = "needs Python with opentimelineio (set OTIO_PYTHON)"]
fn cuts_3_timeline_round_trips_through_opentimelineio() {
    let python = std::env::var("OTIO_PYTHON").unwrap_or_else(|_| "python3".into());
    let probe_import = Command::new(&python)
        .args(["-c", "import opentimelineio"])
        .output();
    if !probe_import.map(|o| o.status.success()).unwrap_or(false) {
        eprintln!("skipping: {python} cannot import opentimelineio");
        return;
    }

    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../editspec/tests/data/cuts-3");
    let dir = tempfile::tempdir().unwrap();
    for f in ["probe.json", "shots.json", "audio.json", "inspect.json"] {
        std::fs::copy(data.join(f), dir.path().join(f)).unwrap();
    }
    let files = exports::run(dir.path()).unwrap();
    let otio_file = files
        .iter()
        .find(|f| f.extension().is_some_and(|e| e == "otio"))
        .unwrap();

    let out = Command::new(&python)
        .args(["-c", CHECK, otio_file.to_str().unwrap(), "180"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    eprintln!(
        "opentimelineio read it back: {}",
        String::from_utf8_lossy(&out.stdout).trim()
    );
}

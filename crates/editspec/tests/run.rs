use std::fs;
use std::path::Path;

#[test]
fn run_reads_the_four_files_and_writes_edit_spec_md() {
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/cuts-3");
    let dir = tempfile::tempdir().unwrap();
    for f in ["probe.json", "shots.json", "audio.json", "inspect.json"] {
        fs::copy(data.join(f), dir.path().join(f)).unwrap();
    }

    let path = editspec::run(dir.path()).unwrap();

    assert_eq!(path, dir.path().join("EDIT-SPEC.md"));
    let doc = fs::read_to_string(path).unwrap();
    assert!(doc.starts_with("# Edit spec: cuts-3.mp4"));
}

#[test]
fn a_missing_measurement_file_is_named_in_the_error() {
    let dir = tempfile::tempdir().unwrap();
    let err = editspec::run(dir.path()).unwrap_err();
    assert!(err.to_string().contains("probe.json"), "{err}");
}

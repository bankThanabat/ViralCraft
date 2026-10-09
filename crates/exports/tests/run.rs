use std::fs;
use std::path::Path;

#[test]
fn run_writes_both_caption_files() {
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../editspec/tests/data/cuts-3");
    let dir = tempfile::tempdir().unwrap();
    for f in ["probe.json", "audio.json", "inspect.json"] {
        fs::copy(data.join(f), dir.path().join(f)).unwrap();
    }

    let files = exports::run(dir.path()).unwrap();

    assert_eq!(
        files,
        [
            dir.path().join("captions.vtt"),
            dir.path().join("captions.ass")
        ]
    );
    exports::vtt::validate(&fs::read_to_string(&files[0]).unwrap()).unwrap();
    assert!(fs::read_to_string(&files[1])
        .unwrap()
        .starts_with("[Script Info]"));
}

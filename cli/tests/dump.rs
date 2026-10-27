//! Integration test for the offline `dump` command: it must read a codeplug
//! `.bin` from disk, parse it, and emit JSON with `channels` and `zones` keys.
//! Uses an all-zero (valid-size) synthetic image so no real-radio data or
//! hardware is involved.

use std::fs;
use std::process::Command;

/// Running `dump` on a correctly sized, all-empty codeplug prints JSON with
/// empty channel and zone lists and exits successfully.
#[test]
fn dump_prints_json_for_empty_codeplug() {
    let dir = std::env::temp_dir().join(format!("anytone-dump-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("empty.bin");
    fs::write(&path, vec![0u8; codeplugme_core::codeplug_size()]).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_codeplugme-cli"))
        .arg("dump")
        .arg(&path)
        .output()
        .expect("failed to run codeplugme-cli");

    assert!(output.status.success(), "dump should exit successfully");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\"channels\""), "JSON should list channels");
    assert!(stdout.contains("\"zones\""), "JSON should list zones");

    let _ = fs::remove_dir_all(&dir);
}

/// `dump` on a wrong-size file fails cleanly rather than panicking.
#[test]
fn dump_rejects_wrong_size_file() {
    let dir = std::env::temp_dir().join(format!("anytone-dump-bad-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bad.bin");
    fs::write(&path, vec![0u8; 32]).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_codeplugme-cli"))
        .arg("dump")
        .arg(&path)
        .output()
        .expect("failed to run codeplugme-cli");

    assert!(
        !output.status.success(),
        "dump should fail on a wrong-size file"
    );

    let _ = fs::remove_dir_all(&dir);
}

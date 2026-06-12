use std::process::Command;

use serde_json::Value;

fn run_winr(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_winr"))
        .args(args)
        .output()
        .expect("failed to run winr")
}

#[test]
fn windows_list_json_is_valid() {
    let output = run_winr(&["--json", "windows", "list"]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(json["ok"], true);
    assert!(json["data"].is_array());
}

#[test]
fn window_info_reports_not_found() {
    let output = run_winr(&[
        "--json",
        "window",
        "info",
        "--title",
        "__WINR_SHOULD_NOT_EXIST__",
    ]);

    assert!(!output.status.success(), "expected not found failure");
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"], "WindowNotFound");
}

#[test]
fn window_info_can_report_ambiguous_selector() {
    let listing = run_winr(&["--json", "windows", "list"]);
    assert!(
        listing.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&listing.stderr)
    );

    let json: Value = serde_json::from_slice(&listing.stdout).expect("stdout should be valid JSON");
    let windows = json["data"].as_array().expect("data should be an array");
    if windows.len() < 2 {
        return;
    }

    let output = run_winr(&["--json", "window", "info", "--title", ""]);
    assert!(!output.status.success(), "expected ambiguity failure");
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"], "AmbiguousWindow");
    assert!(
        json["matches"]
            .as_array()
            .is_some_and(|matches| !matches.is_empty())
    );
}

#[test]
fn window_restore_reports_not_found() {
    let output = run_winr(&[
        "--json",
        "window",
        "restore",
        "--title",
        "__WINR_SHOULD_NOT_EXIST__",
    ]);

    assert!(!output.status.success(), "expected not found failure");
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"], "WindowNotFound");
}

#[test]
fn window_move_requires_coordinates_and_selector() {
    let output = run_winr(&["window", "move", "--x", "10", "--y", "20"]);
    assert!(!output.status.success(), "expected clap failure");
}

use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

fn run_winr(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_winr"))
        .args(args)
        .output()
        .expect("failed to run winr")
}

fn run_winr_with_config(args: &[&str], config_contents: &str) -> std::process::Output {
    let config_path = temp_config_path();
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).expect("failed to create temp config dir");
    }
    fs::write(&config_path, config_contents).expect("failed to write temp config");

    let output = Command::new(env!("CARGO_BIN_EXE_winr"))
        .args(args)
        .env("WINR_CONFIG", &config_path)
        .output()
        .expect("failed to run winr with temp config");

    let _ = fs::remove_file(&config_path);
    output
}

fn temp_config_path() -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_millis();
    std::env::temp_dir().join(format!("winr-test-config-{millis}.toml"))
}

fn temp_profile_path() -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_millis();
    std::env::temp_dir().join(format!("winr-test-profile-{millis}.toml"))
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

#[test]
fn screenshot_window_reports_not_found() {
    let output = run_winr(&[
        "--json",
        "screenshot",
        "window",
        "--title",
        "__WINR_SHOULD_NOT_EXIST__",
        "--out",
        "target\\missing-window.png",
    ]);

    assert!(!output.status.success(), "expected not found failure");
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"], "WindowNotFound");
}

#[test]
fn input_text_reports_not_found() {
    let output = run_winr(&[
        "--json",
        "input",
        "text",
        "--title",
        "__WINR_SHOULD_NOT_EXIST__",
        "hello",
    ]);

    assert!(!output.status.success(), "expected not found failure");
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"], "WindowNotFound");
}

#[test]
fn input_text_message_mode_reports_not_found() {
    let output = run_winr(&[
        "--json",
        "input",
        "text",
        "--input-mode",
        "message",
        "--title",
        "__WINR_SHOULD_NOT_EXIST__",
        "hello",
    ]);

    assert!(!output.status.success(), "expected not found failure");
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"], "WindowNotFound");
}

#[test]
fn input_keys_message_mode_reports_not_found() {
    let output = run_winr(&[
        "--json",
        "input",
        "keys",
        "--input-mode",
        "message",
        "--title",
        "__WINR_SHOULD_NOT_EXIST__",
        "--combo",
        "ctrl+a",
    ]);

    assert!(!output.status.success(), "expected not found failure");
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"], "WindowNotFound");
}

#[test]
fn input_sequence_message_mode_reports_not_found() {
    let output = run_winr(&[
        "--json",
        "input",
        "sequence",
        "--input-mode",
        "message",
        "--title",
        "__WINR_SHOULD_NOT_EXIST__",
        "--step",
        "ctrl+a",
    ]);

    assert!(!output.status.success(), "expected not found failure");
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"], "WindowNotFound");
}

#[test]
fn mouse_click_window_reports_not_found() {
    let output = run_winr(&[
        "--json",
        "mouse",
        "click-window",
        "--title",
        "__WINR_SHOULD_NOT_EXIST__",
        "--x",
        "10",
        "--y",
        "20",
    ]);

    assert!(!output.status.success(), "expected not found failure");
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"], "WindowNotFound");
}

#[test]
fn screenshot_desktop_honors_permission_config() {
    let output = run_winr_with_config(
        &[
            "--json",
            "screenshot",
            "desktop",
            "--out",
            "target\\denied-desktop.png",
        ],
        r#"
[permissions]
allow_input = true
allow_mouse = true
allow_screenshots = false
allow_window_close = false
require_confirm_for_close = true
"#,
    );

    assert!(!output.status.success(), "expected permission failure");
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"], "PermissionDenied");
}

#[test]
fn mouse_click_requires_both_coordinates_in_json_mode() {
    let output = run_winr(&["--json", "mouse", "click", "--x", "10"]);

    assert!(!output.status.success(), "expected validation failure");
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"], "Unsupported");
}

#[test]
fn profile_run_times_out_when_target_never_appears() {
    let profile_path = temp_profile_path();
    fs::write(
        &profile_path,
        r#"
[profile]
id = "missing-profile"
name = "Missing Profile"
description = "Used for timeout testing"
version = "1"

[target]
title_contains = "__WINR_SHOULD_NOT_EXIST__"
exe = "RobloxPlayerBeta.exe"

[action]
kind = "mouse_click"
button = "left"

[schedule]
mode = "interval"
every_ms = 50
random_delta_ms = 20
run_until_stopped = true

[logging]
level = "info"
mode = "single_line_counter"
update_every_trigger = true
template = "autoclicks fired: {count}"

[safety]
require_visible_window = true
require_foreground_window = true
stop_on_focus_loss = true
"#,
    )
    .expect("failed to write temp profile");

    let output = run_winr(&[
        "--json",
        "profile",
        "run",
        profile_path.to_str().expect("temp profile path should be valid"),
        "--wait-timeout-ms",
        "1",
    ]);

    let _ = fs::remove_file(&profile_path);

    assert!(!output.status.success(), "expected timeout failure");
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"], "WindowNotFound");
}

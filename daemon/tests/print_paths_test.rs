use serde_json::Value;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn print_paths_outputs_json_and_exits_successfully() {
    let temp = tempdir().unwrap();
    let state = temp.path().join("state");
    let cache = temp.path().join("cache");
    let runtime = temp.path().join("runtime");

    let output = Command::new(env!("CARGO_BIN_EXE_finalbossd"))
        .arg("--print-paths")
        .env("XDG_STATE_HOME", &state)
        .env("XDG_CACHE_HOME", &cache)
        .env("XDG_RUNTIME_DIR", &runtime)
        .output()
        .unwrap();

    assert!(output.status.success());
    let paths: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        paths["socket_path"],
        runtime
            .join("finalboss/backend.sock")
            .to_string_lossy()
            .as_ref()
    );
}

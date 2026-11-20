#[path = "../src/paths.rs"]
mod paths;

use paths::Paths;
use tempfile::tempdir;

#[test]
fn derives_socket_and_database_paths_from_xdg_roots() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    let state = temp.path().join("state");
    let cache = temp.path().join("cache");
    let runtime = temp.path().join("runtime");

    let paths = Paths::from_env_with(&home, Some(&state), Some(&cache), Some(&runtime));

    assert_eq!(paths.socket_path, runtime.join("finalboss/backend.sock"));
    assert_eq!(paths.db_path, state.join("finalboss/library.sqlite"));
}

#[test]
fn detects_paths_from_the_process_environment() {
    let paths = Paths::from_env().unwrap();

    assert_eq!(paths.state_dir.file_name().unwrap(), "finalboss");
}

#[test]
fn falls_back_to_home_for_missing_xdg_roots() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");

    let paths = Paths::from_env_with(&home, None, None, None);

    assert_eq!(paths.state_dir, home.join(".local/state/finalboss"));
    assert_eq!(paths.cache_dir, home.join(".cache/finalboss"));
    assert_eq!(paths.runtime_dir, home.join(".local/state/finalboss"));
}

#[test]
fn ensure_creates_all_required_directories() {
    let temp = tempdir().unwrap();
    let paths = Paths::from_env_with(
        temp.path(),
        Some(&temp.path().join("state")),
        Some(&temp.path().join("cache")),
        Some(&temp.path().join("runtime")),
    );

    paths.ensure().unwrap();

    assert!(paths.state_dir.is_dir());
    assert!(paths.cache_dir.is_dir());
    assert!(paths.runtime_dir.is_dir());
    assert!(paths.art_dir.is_dir());
}

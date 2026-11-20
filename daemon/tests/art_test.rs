#[allow(dead_code)]
#[path = "../src/art.rs"]
mod art;

use tempfile::tempdir;

#[test]
fn cache_path_is_stable_for_album_id() {
    let temp = tempdir().unwrap();
    let art_dir = temp.path().join("art");

    assert_eq!(art::cache_path(&art_dir, 42), art_dir.join("album-42.jpg"));
    assert_eq!(art::cache_path(&art_dir, 42), art::cache_path(&art_dir, 42));
    assert_ne!(art::cache_path(&art_dir, 42), art::cache_path(&art_dir, 43));
}

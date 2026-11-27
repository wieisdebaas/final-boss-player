#[allow(dead_code)]
#[path = "../src/art.rs"]
mod art;
#[allow(dead_code)]
#[path = "../src/db.rs"]
mod db;
#[allow(dead_code)]
#[path = "../src/meta.rs"]
mod meta;
#[allow(dead_code)]
#[path = "../src/scan.rs"]
mod scan;

use tempfile::tempdir;

#[test]
fn cache_path_is_stable_for_album_id() {
    let temp = tempdir().unwrap();
    let art_dir = temp.path().join("art");

    assert_eq!(art::cache_path(&art_dir, 42), art_dir.join("album-42.jpg"));
    assert_eq!(art::cache_path(&art_dir, 42), art::cache_path(&art_dir, 42));
    assert_ne!(art::cache_path(&art_dir, 42), art::cache_path(&art_dir, 43));
    assert_eq!(
        art::artist_cache_path(&art_dir, 42),
        art_dir.join("artist-42.jpg")
    );
    assert_ne!(
        art::cache_path(&art_dir, 42),
        art::artist_cache_path(&art_dir, 42)
    );
}

#[test]
fn stalled_tag_read_times_out_instead_of_hanging() {
    // Opening a FIFO with no writer blocks forever, like a dead network mount.
    let temp = tempdir().unwrap();
    let fifo = temp.path().join("stalled.flac");
    let c_path = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
    let io = scan::TimedIo::new(1, std::time::Duration::from_millis(150));

    let started = std::time::Instant::now();
    let error = art::read_picture(&io, &fifo).unwrap_err();

    assert!(error.to_string().contains("timed out"), "{error}");
    assert!(started.elapsed() < std::time::Duration::from_secs(3));

    // Release the stranded worker so the test process exits cleanly.
    let _writer = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&fifo)
        .unwrap();
}

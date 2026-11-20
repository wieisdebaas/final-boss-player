# Endboss Music Player

Endboss is an Omarchy Quickshell music player plugin backed by the `endbossd`
Rust daemon. The plugin id is `io.github.endboss.player`.

Run the daemon:

```sh
cd daemon
cargo run
```

Print the resolved state, cache, runtime, database, artwork, and socket paths:

```sh
cd daemon
cargo run -- --print-paths
```

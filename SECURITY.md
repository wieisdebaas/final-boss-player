# Security

## Scope

Final Boss Player indexes and streams files from paths you configure. Online
metadata lookup (MusicBrainz, Cover Art Archive, and AcoustID) runs only when
**Update metadata while scanning** is on. It sends album and artist names, and
for files without those tags a short audio fingerprint. It does not upload the
audio itself.

## Reporting

Please open a private GitHub security advisory on
[wieisdebaas/final-boss-player](https://github.com/wieisdebaas/final-boss-player)
or email the maintainer listed in the repository profile. Do not file public
issues for unfixed vulnerabilities.

## Notes for operators

- Only add music folders you trust; the daemon reads those trees and embedded tags.
- The JSON-RPC socket is user-local (`XDG_RUNTIME_DIR`); do not expose it on the network.
- Artwork thumbnails are cached under `~/.cache/finalboss/`.

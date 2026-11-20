# Security

## Scope

End Boss Player indexes and streams files from paths you configure. It does not
talk to cloud services or scrape online metadata.

## Reporting

Please open a private GitHub security advisory on
[wieisdebaas/endboss-player](https://github.com/wieisdebaas/endboss-player)
or email the maintainer listed in the repository profile. Do not file public
issues for unfixed vulnerabilities.

## Notes for operators

- Only add music folders you trust; the daemon reads those trees and embedded tags.
- The JSON-RPC socket is user-local (`XDG_RUNTIME_DIR`); do not expose it on the network.
- Artwork thumbnails are cached under `~/.cache/endboss/`.

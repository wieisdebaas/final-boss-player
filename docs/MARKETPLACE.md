# Omarchy Marketplace listing

Use these fields when submitting Final Boss Player to the Omarchy plugin
marketplace. They match how other media plugins are tagged so people can find
a music / NAS player.

## Listing fields

| Field | Value |
| --- | --- |
| **Name** | Final Boss Player |
| **Plugin id** | `io.github.finalboss.player` |
| **Repository** | `https://github.com/wieisdebaas/finalboss-player.git` |
| **Homepage** | `https://github.com/wieisdebaas/finalboss-player` |
| **License** | MIT |
| **Category** | `Desktop` |
| **Tags** (max 3) | `media`, `quickshell`, `bar` |
| **Short description** | Stream your NAS and local music library from the Omarchy bar and panel. |
| **Preview image** | `preview.png` (repo root) |
| **Icon** | `icon.png` (repo root) |

### Why these tags

Scanned against published Omarchy marketplace guidance and similar plugins:

| Tag | Why |
| --- | --- |
| `media` | Music / audio player discovery |
| `quickshell` | Shell / panel plugin (not Electron) |
| `bar` | Ships a bar mini-player widget |

**Category `Desktop`** is preferred over a shell-only “Widgets” bucket so the
listing reads as a music app. Official marketplace categories include Appearance,
Desktop, Developer Tools, Hardware, Kids, Productivity, System, Widgets, Other.

Official marketplace tags (max 3): `ai`, `bar`, `education`, `games`, `hyprland`,
`kids`, `launcher`, `media`, `power-management`, `quickshell`, `security`,
`system`, `vpn`, `workspaces`.

The plugin’s own `manifest.json` `barWidget.category` stays **`Media`** for the
Omarchy bar picker (that value is separate from marketplace category).

Optional: suggest missing tag `music` or `audio` on the submission form.

### Install note for the listing description

`omarchy plugin add` installs the QML plugin only. Consumers must also run
`scripts/install.sh` once to build `finalbossd` and enable the systemd user unit.
Put that in the marketplace long description (see README Install section).

### Submit

```sh
gh issue create \
  --repo omacom/omarchy-plugin-marketplace \
  --title "[Plugin]: Final Boss Player" \
  --body-file docs/marketplace-submission.md
```

Use `docs/marketplace-submission.md` after the default branch has this README,
`LICENSE`, `preview.png`, and a valid root `manifest.json`.

## Shell findability (`manifest.json`)

These are **not** marketplace tags; they drive Omarchy’s plugin / bar search:

```json
"aliases": [
  "music",
  "player",
  "finalboss",
  "final boss",
  "nas",
  "audio",
  "library",
  "mpris",
  "flac",
  "local-music"
]
```

Keep `barWidget.category` as `"Media"`.

## Suggested GitHub repository topics

```
omarchy quickshell music-player nas hyprland qml rust linux mpris flac
```

Set with:

```sh
gh repo edit wieisdebaas/finalboss-player --add-topic omarchy --add-topic quickshell \
  --add-topic music-player --add-topic nas --add-topic hyprland --add-topic qml \
  --add-topic rust --add-topic linux --add-topic mpris --add-topic flac
```

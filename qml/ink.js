.pragma library

// Theme "muted" can sit almost on the background. This stays on the foreground
// side, so artist, album, and genre stay readable on a dark grey page.
function secondary(foreground, background) {
  return Qt.rgba(
    foreground.r * 0.78 + background.r * 0.22,
    foreground.g * 0.78 + background.g * 0.22,
    foreground.b * 0.78 + background.b * 0.22,
    1
  )
}

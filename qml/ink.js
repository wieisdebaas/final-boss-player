.pragma library

// Three text steps mixed from the theme, so a different Omarchy theme still
// shifts them. Theme "muted" sits too close to the background to use for type.
function mix(foreground, background, amount) {
  var rest = 1 - amount
  return Qt.rgba(
    foreground.r * amount + background.r * rest,
    foreground.g * amount + background.g * rest,
    foreground.b * amount + background.b * rest,
    1)
}

// Page names, song titles, sidebar, fields.
function title(foreground, background) { return mix(foreground, background, 0.85) }

// Counts, artist / album / genre, unselected chips, durations.
function meta(foreground, background) { return mix(foreground, background, 0.62) }

// Placeholders, long help, empty states.
function quiet(foreground, background) { return mix(foreground, background, 0.48) }

// Unselected chip ring. Solid, so it stays visible on the page.
function ring(foreground, background) { return mix(foreground, background, 0.413) }

.pragma library

function columnsOf(tiled, columnCount) {
  return tiled ? Math.max(1, columnCount | 0) : 1
}

// edge is "", "start", "end", or "empty". "start" / "end" mean the move
// would leave this list, so the caller can continue into a neighbor.
function move(index, count, columns, dx, dy) {
  if (count <= 0) return { index: -1, edge: "empty" }
  var cols = Math.max(1, columns | 0)
  if (index < 0) {
    if (dx < 0 || dy < 0) return { index: count - 1, edge: "" }
    return { index: 0, edge: "" }
  }
  var col = index % cols
  var row = Math.floor(index / cols)
  var lastRow = Math.floor((count - 1) / cols)
  if (dy < 0 && row === 0) return { index: index, edge: "start" }
  if (dy > 0 && row === lastRow) return { index: index, edge: "end" }
  var nextCol = col + (dx | 0)
  if (nextCol < 0) nextCol = 0
  if (nextCol >= cols) nextCol = cols - 1
  var nextRow = row + (dy | 0)
  if (nextRow < 0) nextRow = 0
  if (nextRow > lastRow) nextRow = lastRow
  var next = nextRow * cols + nextCol
  if (next >= count) next = count - 1
  if (next < 0) next = 0
  return { index: next, edge: "" }
}

function page(index, count, columns, rowsPerPage, direction) {
  if (count <= 0) return -1
  var cols = Math.max(1, columns | 0)
  var step = Math.max(1, rowsPerPage | 0) * cols * direction
  var start = index < 0 ? 0 : index
  var next = start + step
  if (next < 0) next = 0
  if (next >= count) next = count - 1
  return next
}

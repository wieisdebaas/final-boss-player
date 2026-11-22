import QtQuick

// Shared brain for the bar widget and the lazy full panel. It owns the single
// finalbossd connection and exposes the latest player state plus thin command
// wrappers. UI components never talk to the socket directly.
Item {
  id: root

  visible: false
  width: 0
  height: 0

  // Injected by the Omarchy host.
  property var shell: null
  property var manifest: null
  property var pluginRegistry: null

  readonly property string pluginId: manifest && manifest.id
    ? String(manifest.id) : "io.github.finalboss.player"

  readonly property var pages: ["home", "search", "library", "artists", "albums", "settings"]
  property string currentPage: "home"

  // Drill-down selection for the albums/artists pages (null = show the list).
  property var selectedAlbum: null
  property var selectedArtist: null
  // Search state lives here so Ctrl+F can focus it from anywhere.
  property string searchQuery: ""
  property int searchFocusRequest: 0

  readonly property var emptyState: ({
    lifecycle: "disconnected",
    playing: false,
    track: null,
    position_ms: 0,
    duration_ms: null,
    volume: 1.0,
    shuffle: false,
    repeat: "off",
    scan: null,
    folders: [],
    error_code: null,
    error_message: null,
    generation: 0
  })

  // Latest snapshot pushed by the daemon (emptyState while disconnected).
  property var playerState: emptyState
  readonly property bool connected: backend.connected
  readonly property bool playing: playerState.playing === true
  readonly property bool shuffle: playerState.shuffle === true
  readonly property var track: playerState.track || null
  readonly property string title: track && track.title ? String(track.title) : ""
  readonly property string artist: track && track.artist ? String(track.artist) : ""
  readonly property bool hasMedia: track !== null
  readonly property var folders: playerState.folders || []
  readonly property bool scanning: !!(playerState.scan && playerState.scan.scanning)
  readonly property string errorMessage: playerState.error_message
    ? String(playerState.error_message) : ""

  // Bumped whenever a scan finishes so library views can re-query.
  property int libraryRevision: 0
  property bool uiVisible: false

  signal operationFailed(string reason)

  function navigate(page) {
    var next = String(page || "")
    if (pages.indexOf(next) < 0) return false
    // Re-selecting a sidebar entry returns to its top-level list.
    selectedAlbum = null
    selectedArtist = null
    currentPage = next
    return true
  }

  function openAlbum(album) {
    selectedAlbum = album || null
    currentPage = "albums"
  }

  function openArtist(artist) {
    selectedArtist = artist || null
    currentPage = "artists"
  }

  function openAlbumFromTrack(track) {
    if (!track || track.album_id === undefined || track.album_id === null) return false
    openAlbum({
      id: Number(track.album_id),
      name: track.album || "Unknown album",
      album_artist: track.album_artist || track.artist || null
    })
    return true
  }

  function openArtistFromTrack(track) {
    if (!track || track.artist_id === undefined || track.artist_id === null) return false
    openArtist({
      id: Number(track.artist_id),
      name: track.artist || "Unknown artist"
    })
    return true
  }

  function focusSearch() {
    currentPage = "search"
    searchFocusRequest++
  }

  // Relative controls used by keyboard shortcuts.
  function seekBy(deltaMs) {
    if (!connected || !hasMedia) return
    var duration = Number(playerState.duration_ms) || 0
    var target = (Number(playerState.position_ms) || 0) + deltaMs
    if (duration > 0) target = Math.min(target, duration)
    seek(target)
  }

  function volumeBy(delta) {
    if (!connected) return
    setVolume((Number(playerState.volume) || 0) + delta)
  }

  property real volumeBeforeMute: 0.8
  function toggleMute() {
    if (!connected) return
    var current = Number(playerState.volume) || 0
    if (current > 0) {
      volumeBeforeMute = current
      setVolume(0)
    } else {
      setVolume(volumeBeforeMute > 0 ? volumeBeforeMute : 0.8)
    }
  }

  function setUiVisible(visible) {
    uiVisible = visible === true
  }

  // Force a fresh connection attempt (e.g. after starting finalbossd).
  function reconnect() {
    backend.wanted = false
    backend.wanted = true
  }

  // Generic request. callback(ok, result, errorMessage) is optional.
  function request(command, fields, callback) {
    return backend.send(command, fields, function(ok, result, error) {
      if (!ok && error) root.operationFailed(error)
      if (typeof callback === "function") callback(ok, result, error)
    })
  }

  function query(kind, q, limit, offset, callback) {
    return request("query", {
      kind: String(kind || "tracks"),
      q: String(q || ""),
      limit: limit === undefined ? 100 : limit,
      offset: offset === undefined ? 0 : offset
    }, callback)
  }

  // scope is "album", "artist", or "library": the queue shuffle and next walk.
  function play(trackId, scope, callback) {
    return request("play", {
      track_id: trackId,
      scope: scope || "album"
    }, callback)
  }
  // Same track toggles play/pause; a different track starts it in `scope`.
  function playOrToggle(trackId, scope, callback) {
    var id = Number(trackId)
    if (track && Number(track.id) === id) {
      toggle()
      if (typeof callback === "function") callback(true, null, null)
      return true
    }
    return play(id, scope, callback)
  }
  function playAlbum(albumId, callback) {
    return request("play_album", { album_id: albumId }, callback)
  }
  function addFolder(path, callback) {
    return request("add_folder", { path: String(path || "") }, callback)
  }
  function removeFolder(path, callback) {
    return request("remove_folder", { path: String(path || "") },
      function(ok, result, error) {
        if (ok) root.libraryRevision++
        if (typeof callback === "function") callback(ok, result, error)
      })
  }
  function rescan(full, callback) {
    return request("rescan", { full: full === true }, callback)
  }
  function toggle() { return request("toggle", null, null) }
  function pause() { return request("pause", null, null) }
  function next() { return request("next", null, null) }
  function previous() { return request("previous", null, null) }
  function seek(positionMs) {
    return request("seek", { position_ms: Math.max(0, Math.floor(positionMs)) }, null)
  }
  function setVolume(level) {
    return request("set_volume", { level: Math.max(0, Math.min(1, level)) }, null)
  }
  function setShuffle(enabled) {
    return request("set_shuffle", { enabled: enabled === true }, null)
  }
  function toggleShuffle() {
    return setShuffle(!shuffle)
  }
  function setRepeat(mode) {
    return request("set_repeat", { mode: String(mode || "off") }, null)
  }

  function applyState(next) {
    var wasScanning = scanning
    playerState = next
    if (wasScanning && !scanning) libraryRevision++
  }

  BackendClient {
    id: backend
    wanted: true
    onStateReceived: function(state) { root.applyState(state) }
  }

  Connections {
    target: backend
    function onConnectedChanged() {
      if (backend.connected) root.libraryRevision++
      else root.playerState = root.emptyState
    }
  }
}

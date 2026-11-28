pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import qs.Commons
import "listkeys.js" as ListKeys
import "ink.js" as Ink

// Reusable track rows, or cover tiles when the service is in tiles mode.
// Row / title click plays/toggles; artist, album, and genre links only activate
// when the pointer is over their text. A tile click plays the track.
Item {
  id: root

  property var service: null
  property var tracks: []
  property string emptyText: "No tracks"
  property bool showNumbers: true
  // Small album cover on each row. Tiles always show art.
  property bool covers: false
  // Queue built when a row is played: "album", "artist", "library", "genre", or "search".
  property string queueScope: "album"
  // Which footer jump this list answers: "albums", "artists", or "library".
  property string revealPage: ""

  readonly property int currentId: service && service.track && service.track.id !== undefined
    ? Number(service.track.id) : -1
  readonly property bool tiled: !!(service && service.tiles)
  readonly property int columnCount: Math.max(2, Math.floor(Math.max(width, 1) / 156))
  readonly property int cellWidth: Math.max(1, Math.floor(width / columnCount))
  readonly property int cellHeight: cellWidth + 52

  function formatDuration(ms) {
    var total = Math.max(0, Math.floor(Number(ms) / 1000))
    var seconds = total % 60
    return Math.floor(total / 60) + ":" + (seconds < 10 ? "0" : "") + seconds
  }

  function playTrack(track) {
    if (!service) return
    if (queueScope === "search") {
      var ids = []
      var rows = tracks || []
      for (var i = 0; i < rows.length; ++i) ids.push(Number(rows[i].id))
      service.playList(Number(track.id), ids, null)
      return
    }
    service.playOrToggle(Number(track.id), queueScope, null)
  }

  property int keyboardIndex: -1
  property bool keyboardFocused: false
  readonly property int keyboardCount: Array.isArray(tracks) ? tracks.length : 0
  readonly property int rowHeight: covers ? 52 : 44
  readonly property color titleColor: Ink.title(Color.foreground, Color.background)
  readonly property color metaColor: Ink.meta(Color.foreground, Color.background)
  readonly property color quietColor: Ink.quiet(Color.foreground, Color.background)
  readonly property int keyboardStride: tiled ? cellHeight : rowHeight

  function publishKeyboard() {
    if (keyboardIndex >= keyboardCount)
      keyboardIndex = keyboardCount > 0 ? keyboardCount - 1 : -1
    if (service) service.noteKeyboard()
  }

  function revealKeyboard() {
    if (keyboardIndex < 0) return
    var view = tiled ? grid : rows
    view.positionViewAtIndex(keyboardIndex, tiled ? GridView.Contain : ListView.Contain)
  }

  property int seenReveal: 0

  function trackIndex(id) {
    if (id < 0 || !Array.isArray(tracks)) return -1
    for (var i = 0; i < tracks.length; ++i) {
      if (Number(tracks[i].id) === id) return i
    }
    return -1
  }

  function isShown() {
    var item = root
    while (item) {
      if (!item.visible) return false
      item = item.parent
    }
    return true
  }

  // Scroll the playing track into the middle once this list is the open page.
  function maybeRevealPlaying() {
    if (!service || revealPage === "" || revealPage !== service.playingRevealPage) return
    var token = service.playingReveal
    if (token === 0 || token === seenReveal) return
    if (!isShown()) return
    var index = trackIndex(currentId)
    if (index < 0) return
    seenReveal = token
    keyboardIndex = index
    var view = tiled ? grid : rows
    view.positionViewAtIndex(index, tiled ? GridView.Center : ListView.Center)
  }

  function scheduleReveal() {
    Qt.callLater(function() {
      Qt.callLater(function() { root.maybeRevealPlaying() })
    })
  }

  Connections {
    target: root.service
    function onPlayingRevealChanged() { root.scheduleReveal() }
  }
  onTracksChanged: scheduleReveal()

  function moveKeyboard(dx, dy) {
    var result = ListKeys.move(keyboardIndex, keyboardCount,
      ListKeys.columnsOf(tiled, columnCount), dx, dy)
    if (result.edge === "") {
      keyboardIndex = result.index
      revealKeyboard()
    }
    return result.edge
  }

  function pageKeyboard(direction) {
    var rowsHigh = Math.max(1, Math.floor(height / Math.max(1, keyboardStride)))
    keyboardIndex = ListKeys.page(keyboardIndex, keyboardCount,
      ListKeys.columnsOf(tiled, columnCount), rowsHigh, direction)
    revealKeyboard()
  }

  function jumpKeyboardStart() {
    keyboardIndex = keyboardCount > 0 ? 0 : -1
    revealKeyboard()
  }

  function jumpKeyboardEnd() {
    keyboardIndex = keyboardCount > 0 ? keyboardCount - 1 : -1
    revealKeyboard()
  }

  function activateKeyboard() {
    if (keyboardIndex < 0 || keyboardIndex >= keyboardCount) return
    playTrack(tracks[keyboardIndex])
  }

  onKeyboardCountChanged: publishKeyboard()
  onVisibleChanged: {
    scheduleReveal()
    publishKeyboard()
  }
  onTiledChanged: {
    revealKeyboard()
    publishKeyboard()
  }

  ListView {
    id: rows
    anchors.fill: parent
    visible: !root.tiled
    clip: true
    model: root.tiled ? [] : root.tracks
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar {
      policy: ScrollBar.AsNeeded
      width: 10
      contentItem: Rectangle {
        implicitWidth: 6
        radius: 3
        color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b,
                       parent.hovered || parent.pressed ? 0.45 : 0.28)
      }
    }

  delegate: Rectangle {
    id: row
    required property var modelData
    required property int index
    readonly property bool isCurrent: Number(modelData.id) === root.currentId
    readonly property bool keyboardHere: root.keyboardFocused && index === root.keyboardIndex
    readonly property bool hasArtist: !!(modelData.artist && modelData.artist_id !== undefined
      && modelData.artist_id !== null)
    readonly property bool hasAlbum: !!(modelData.album && modelData.album_id !== undefined
      && modelData.album_id !== null)
    readonly property bool hasGenre: !!(modelData.genre && String(modelData.genre).trim() !== "")

    property string artPath: ""
    property int artAlbumId: -1

    function refreshArt() {
      if (!root.covers || !root.service || !root.service.connected || row.modelData.has_art !== true) {
        artPath = ""
        artAlbumId = -1
        return
      }
      var albumId = Number(row.modelData.album_id)
      if (!(albumId >= 0)) return
      if (albumId === artAlbumId && artPath !== "") return
      artAlbumId = albumId
      artPath = ""
      root.service.request("get_art", { album_id: albumId }, function(ok, result) {
        if (albumId !== row.artAlbumId) return
        row.artPath = ok && result && result.path ? String(result.path) : ""
      })
    }

    onModelDataChanged: refreshArt()
    Component.onCompleted: refreshArt()

    width: ListView.view.width
    height: root.rowHeight
    radius: 6
    border.width: row.keyboardHere ? 1 : 0
    border.color: Color.accent
    color: row.keyboardHere
      ? Qt.rgba(Color.accent.r, Color.accent.g, Color.accent.b, 0.2)
      : (rowArea.containsMouse
        ? Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.08)
        : "transparent")

    // Whole-row click plays the title. Artist, album, and genre only steal
    // events when the pointer is over their actual text.
    MouseArea {
      id: rowArea
      anchors.fill: parent
      hoverEnabled: true
      cursorShape: Qt.PointingHandCursor
      onClicked: root.playTrack(row.modelData)
    }

    Row {
      anchors.fill: parent
      anchors.leftMargin: 10
      anchors.rightMargin: 14
      spacing: 12
      // Let rowArea receive clicks in empty space; nested areas still win on top.
      z: 1

      // Always reserve this column so the current track can show play/pause
      // even when showNumbers is false (Search, Home, Artists).
      Text {
        width: 28
        anchors.verticalCenter: parent.verticalCenter
        horizontalAlignment: Text.AlignRight
        color: row.isCurrent ? Color.accent : root.metaColor
        font.pixelSize: 12
        text: row.isCurrent && root.service && root.service.playing
          ? "\u23F8"
          : row.isCurrent
            ? "\u25B6"
            : root.showNumbers
              ? String(row.modelData.track_no > 0 ? row.modelData.track_no : row.index + 1)
              : ""
      }

      Rectangle {
        id: cover
        visible: root.covers
        anchors.verticalCenter: parent.verticalCenter
        width: 36
        height: 36
        radius: 4
        clip: true
        color: Qt.rgba(Color.accent.r, Color.accent.g, Color.accent.b, 0.18)

        Image {
          anchors.fill: parent
          visible: row.artPath !== ""
          source: row.artPath !== "" ? "file://" + row.artPath : ""
          fillMode: Image.PreserveAspectCrop
          sourceSize.width: 72
          sourceSize.height: 72
          asynchronous: true
          cache: true
        }

        Text {
          anchors.centerIn: parent
          visible: row.artPath === ""
          color: Color.accent
          font.pixelSize: 15
          font.bold: true
          text: String(row.modelData.album || row.modelData.title || "?").charAt(0).toUpperCase()
        }
      }

      Column {
        anchors.verticalCenter: parent.verticalCenter
        width: parent.width - 40 - 60 - 24 - (cover.visible ? 48 : 0)
        spacing: 1

        Text {
          width: parent.width
          elide: Text.ElideRight
          color: row.isCurrent ? Color.accent : root.titleColor
          font.pixelSize: 13
          text: String(row.modelData.title || "Untitled")
        }

        Row {
          id: byline
          spacing: 0
          width: parent.width
          // Each visible name gets an equal share, and only as wide as its text,
          // so a long artist does not hide the album or the genre.
          readonly property int pieceCount: (artistLink.visible ? 1 : 0)
            + (albumLink.visible ? 1 : 0) + (genreLink.visible ? 1 : 0)
          readonly property int sepBudget: (artistSep.visible ? artistSep.implicitWidth : 0)
            + (albumSep.visible ? albumSep.implicitWidth : 0)
          readonly property int pieceWidth: pieceCount > 0
            ? Math.max(0, (width - sepBudget) / pieceCount) : 0

          Text {
            id: artistLink
            elide: Text.ElideRight
            maximumLineCount: 1
            width: Math.min(implicitWidth, byline.pieceWidth)
            color: artistArea.containsMouse ? Color.accent : root.metaColor
          font.pixelSize: 12
          font.underline: artistArea.containsMouse
            text: String(row.modelData.artist || "")
            visible: text !== ""
            MouseArea {
              id: artistArea
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: row.hasArtist ? Qt.PointingHandCursor : Qt.ArrowCursor
              enabled: row.hasArtist
              onClicked: if (root.service) root.service.openArtistFromTrack(row.modelData)
            }
          }

          Text {
            id: artistSep
            color: root.metaColor
            font.pixelSize: 12
            text: " \u00B7 "
            visible: artistLink.visible && (albumLink.visible || genreLink.visible)
          }

          Text {
            id: albumLink
            elide: Text.ElideRight
            maximumLineCount: 1
            width: Math.min(implicitWidth, byline.pieceWidth)
            color: albumArea.containsMouse ? Color.accent : root.metaColor
          font.pixelSize: 12
          font.underline: albumArea.containsMouse
            text: String(row.modelData.album || "")
            visible: text !== ""
            MouseArea {
              id: albumArea
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: row.hasAlbum ? Qt.PointingHandCursor : Qt.ArrowCursor
              enabled: row.hasAlbum
              onClicked: if (root.service) root.service.openAlbumFromTrack(row.modelData)
            }
          }

          Text {
            id: albumSep
            color: root.metaColor
            font.pixelSize: 12
            text: " \u00B7 "
            visible: albumLink.visible && genreLink.visible
          }

          Text {
            id: genreLink
            elide: Text.ElideRight
            maximumLineCount: 1
            width: Math.min(implicitWidth, byline.pieceWidth)
            color: genreArea.containsMouse ? Color.accent : root.metaColor
          font.pixelSize: 12
          font.underline: genreArea.containsMouse
            text: String(row.modelData.genre || "")
            visible: text !== ""
            MouseArea {
              id: genreArea
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: row.hasGenre ? Qt.PointingHandCursor : Qt.ArrowCursor
              enabled: row.hasGenre
              onClicked: if (root.service) root.service.openGenreFromTrack(row.modelData)
            }
          }
        }
      }

      Text {
        width: 60
        anchors.verticalCenter: parent.verticalCenter
        horizontalAlignment: Text.AlignRight
        color: root.metaColor
        font.pixelSize: 12
        text: root.formatDuration(row.modelData.duration_ms)
      }
    }
    }
  }

  GridView {
    id: grid
    anchors.fill: parent
    visible: root.tiled
    clip: true
    model: root.tiled ? root.tracks : []
    cellWidth: root.cellWidth
    cellHeight: root.cellHeight
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar {
      policy: ScrollBar.AsNeeded
      width: 10
      contentItem: Rectangle {
        implicitWidth: 6
        radius: 3
        color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b,
                       parent.hovered || parent.pressed ? 0.45 : 0.28)
      }
    }

    delegate: Item {
      id: tile
      required property var modelData
      required property int index
      readonly property bool isCurrent: Number(modelData.id) === root.currentId
      readonly property bool keyboardHere: root.keyboardFocused && index === root.keyboardIndex

      width: GridView.view.cellWidth
      height: GridView.view.cellHeight

      property string artPath: ""
      property int artAlbumId: -1

      function refreshArt() {
        if (!root.service || !root.service.connected || tile.modelData.has_art !== true) {
          artPath = ""
          artAlbumId = -1
          return
        }
        var albumId = Number(tile.modelData.album_id)
        if (!(albumId >= 0)) return
        if (albumId === artAlbumId) return
        artAlbumId = albumId
        artPath = ""
        root.service.request("get_art", { album_id: albumId }, function(ok, result) {
          if (albumId !== tile.artAlbumId) return
          tile.artPath = ok && result && result.path ? String(result.path) : ""
        })
      }

      onModelDataChanged: refreshArt()
      Component.onCompleted: refreshArt()

      Rectangle {
        id: card
        anchors.fill: parent
        anchors.margins: 6
        radius: 8
        color: tile.keyboardHere
          ? Qt.rgba(Color.accent.r, Color.accent.g, Color.accent.b, 0.2)
          : (tileArea.containsMouse
            ? Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.08)
            : "transparent")
        border.width: tile.keyboardHere || tile.isCurrent ? 1 : 0
        border.color: Color.accent

        Rectangle {
          id: cover
          anchors.top: parent.top
          anchors.left: parent.left
          anchors.right: parent.right
          anchors.margins: 6
          height: width
          radius: 6
          clip: true
          color: Qt.rgba(Color.accent.r, Color.accent.g, Color.accent.b, 0.18)

          Image {
            anchors.fill: parent
            visible: tile.artPath !== ""
            source: tile.artPath !== "" ? "file://" + tile.artPath : ""
            fillMode: Image.PreserveAspectCrop
            sourceSize.width: 280
            sourceSize.height: 280
            asynchronous: true
            cache: true
          }

          Text {
            anchors.centerIn: parent
            visible: tile.artPath === ""
            color: Color.accent
            font.pixelSize: 22
            font.bold: true
            text: String(tile.modelData.title || "?").charAt(0).toUpperCase()
          }

          Rectangle {
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.margins: 6
            radius: 4
            color: Qt.rgba(0, 0, 0, 0.55)
            width: durationText.implicitWidth + 10
            height: durationText.implicitHeight + 4
            Text {
              id: durationText
              anchors.centerIn: parent
              color: "white"
              font.pixelSize: 11
              text: root.formatDuration(tile.modelData.duration_ms)
            }
          }
        }

        Text {
          id: titleText
          anchors.top: cover.bottom
          anchors.topMargin: 6
          anchors.left: parent.left
          anchors.right: parent.right
          anchors.leftMargin: 8
          anchors.rightMargin: 8
          elide: Text.ElideRight
          color: tile.isCurrent ? Color.accent : root.titleColor
          font.pixelSize: 13
          text: String(tile.modelData.title || "Untitled")
        }

        Text {
          anchors.top: titleText.bottom
          anchors.left: titleText.left
          anchors.right: titleText.right
          elide: Text.ElideRight
          color: root.metaColor
          font.pixelSize: 12
          text: [tile.modelData.artist, tile.modelData.album, tile.modelData.genre]
            .filter(function(part) { return !!part }).join(" \u00B7 ")
        }

        MouseArea {
          id: tileArea
          anchors.fill: parent
          hoverEnabled: true
          cursorShape: Qt.PointingHandCursor
          onClicked: root.playTrack(tile.modelData)
        }
      }
    }
  }

  Text {
    anchors.centerIn: parent
    visible: !root.tracks || root.tracks.length === 0
    color: root.quietColor
    font.pixelSize: 13
    text: root.emptyText
  }
}

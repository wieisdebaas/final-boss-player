pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import qs.Commons

// Reusable track rows, or cover tiles when the service is in tiles mode.
// Row / title click plays/toggles; artist and album links only activate
// when the pointer is over their text. A tile click plays the track.
Item {
  id: root

  property var service: null
  property var tracks: []
  property string emptyText: "No tracks"
  property bool showNumbers: true
  // Queue built when a row is played: "album", "artist", or "library".
  property string queueScope: "album"

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
    if (service) service.playOrToggle(Number(track.id), queueScope, null)
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
    readonly property bool hasArtist: !!(modelData.artist && modelData.artist_id !== undefined
      && modelData.artist_id !== null)
    readonly property bool hasAlbum: !!(modelData.album && modelData.album_id !== undefined
      && modelData.album_id !== null)

    width: ListView.view.width
    height: 44
    radius: 6
    color: rowArea.containsMouse
      ? Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.08)
      : "transparent"

    // Whole-row click plays the title. Artist/album only steal events when
    // the pointer is over their actual text (not the empty remainder of the row).
    MouseArea {
      id: rowArea
      anchors.fill: parent
      hoverEnabled: true
      cursorShape: Qt.PointingHandCursor
      onClicked: if (root.service)
        root.service.playOrToggle(Number(row.modelData.id), root.queueScope, null)
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
        color: row.isCurrent ? Color.accent : Color.muted
        font.pixelSize: 12
        text: row.isCurrent && root.service && root.service.playing
          ? "\u23F8"
          : row.isCurrent
            ? "\u25B6"
            : root.showNumbers
              ? String(row.modelData.track_no > 0 ? row.modelData.track_no : row.index + 1)
              : ""
      }

      Column {
        anchors.verticalCenter: parent.verticalCenter
        width: parent.width - 40 - 60 - 24
        spacing: 1

        Text {
          width: parent.width
          elide: Text.ElideRight
          color: row.isCurrent ? Color.accent : Color.foreground
          font.pixelSize: 13
          text: String(row.modelData.title || "Untitled")
        }

        Row {
          id: byline
          spacing: 0
          width: parent.width

          Text {
            id: artistLink
            elide: Text.ElideRight
            maximumLineCount: 1
            // Hit box = painted text only (capped so long names don't crush album).
            width: Math.min(implicitWidth, byline.width * 0.5)
            color: artistArea.containsMouse ? Color.accent : Color.muted
            font.pixelSize: 11
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
            id: bylineSep
            color: Color.muted
            font.pixelSize: 11
            text: " \u00B7 "
            visible: artistLink.visible && albumLink.visible
          }

          Text {
            id: albumLink
            elide: Text.ElideRight
            maximumLineCount: 1
            // Same as artist: only as wide as the label, not the rest of the row.
            width: Math.min(
              implicitWidth,
              Math.max(0, byline.width - artistLink.width
                - (bylineSep.visible ? bylineSep.width : 0)))
            color: albumArea.containsMouse ? Color.accent : Color.muted
            font.pixelSize: 11
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
        }
      }

      Text {
        width: 60
        anchors.verticalCenter: parent.verticalCenter
        horizontalAlignment: Text.AlignRight
        color: Color.muted
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
        color: tileArea.containsMouse
          ? Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.08)
          : "transparent"
        border.width: tile.isCurrent ? 1 : 0
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
          color: tile.isCurrent ? Color.accent : Color.foreground
          font.pixelSize: 13
          text: String(tile.modelData.title || "Untitled")
        }

        Text {
          anchors.top: titleText.bottom
          anchors.left: titleText.left
          anchors.right: titleText.right
          elide: Text.ElideRight
          color: Color.muted
          font.pixelSize: 11
          text: [tile.modelData.artist, tile.modelData.album]
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
    color: Color.muted
    font.pixelSize: 13
    text: root.emptyText
  }
}

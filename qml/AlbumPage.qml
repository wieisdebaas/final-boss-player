pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import qs.Commons
import "ink.js" as Ink

// Albums list (query kind "albums"); selecting one shows its tracks
// (kind "album", q = album id) with a Play album button (play_album RPC).
Item {
  id: root

  property var service: null

  property var albums: []
  property var tracks: []
  property bool loading: false
  property string errorText: ""
  property int loadSerial: 0
  readonly property var selected: service ? service.selectedAlbum : null
  readonly property int revision: service ? service.libraryRevision : 0
  readonly property bool connected: service ? service.connected : false
  readonly property var keyboardLists: selected ? [trackList] : [albumList]

  property string artPath: ""
  property int artAlbumId: -1

  function refreshArt() {
    if (!selected || !service || !service.connected || selected.has_art === false) {
      artPath = ""
      artAlbumId = -1
      return
    }
    var albumId = Number(selected.id)
    if (!(albumId >= 0)) return
    if (albumId === artAlbumId) return
    artAlbumId = albumId
    artPath = ""
    service.request("get_art", { album_id: albumId }, function(ok, result) {
      if (!root.selected || Number(root.selected.id) !== albumId) return
      root.artPath = ok && result && result.path ? String(result.path) : ""
    })
  }

  function refresh() {
    root.loadSerial += 1
    var serial = root.loadSerial
    if (!service || !service.connected) {
      albums = []
      tracks = []
      errorText = "finalbossd is not running"
      return
    }
    loading = true
    var showingAlbum = !!selected
    var kind = showingAlbum ? "album" : "albums"
    var q = showingAlbum ? String(selected.id) : ""
    service.queryPages(kind, q, function(ok, result, error) {
      if (serial !== root.loadSerial) return
      root.loading = false
      root.errorText = ok ? "" : error
      if (!ok) return
      if (showingAlbum) root.tracks = result
      else root.albums = result
    })
  }

  onSelectedChanged: {
    tracks = []
    refreshArt()
    refresh()
  }
  onRevisionChanged: refresh()
  onConnectedChanged: {
    refreshArt()
    refresh()
  }
  Component.onCompleted: {
    refreshArt()
    refresh()
  }

  ColumnLayout {
    anchors.fill: parent
    anchors.margins: 20
    spacing: 10

    RowLayout {
      Layout.fillWidth: true
      visible: root.selected === null
      Text {
        Layout.fillWidth: true
        elide: Text.ElideRight
        color: Ink.title(Color.foreground, Color.background)
        font.pixelSize: 20
        font.bold: true
        text: "Albums"
      }
      Text {
        color: Ink.meta(Color.foreground, Color.background)
        font.pixelSize: 12
        text: root.loading ? "Loading\u2026" : root.albums.length + " albums"
      }

      ViewToggle {
        Layout.alignment: Qt.AlignVCenter
        service: root.service
      }
    }

    RowLayout {
      Layout.fillWidth: true
      visible: root.selected !== null
      spacing: 16

      Rectangle {
        Layout.preferredWidth: 112
        Layout.preferredHeight: 112
        Layout.alignment: Qt.AlignVCenter
        radius: 6
        clip: true
        color: Qt.rgba(Color.accent.r, Color.accent.g, Color.accent.b, 0.18)

        Image {
          anchors.fill: parent
          visible: root.artPath !== ""
          source: root.artPath !== "" ? "file://" + root.artPath : ""
          fillMode: Image.PreserveAspectCrop
          sourceSize.width: 224
          sourceSize.height: 224
          asynchronous: true
          cache: true
        }

        Text {
          anchors.centerIn: parent
          visible: root.artPath === ""
          color: Color.accent
          font.pixelSize: 42
          font.bold: true
          text: root.selected ? String(root.selected.name || "?").charAt(0).toUpperCase() : ""
        }
      }

      ColumnLayout {
        Layout.fillWidth: true
        Layout.alignment: Qt.AlignVCenter
        spacing: 4

        Text {
          color: backArea.containsMouse ? Color.accent : Ink.meta(Color.foreground, Color.background)
          font.pixelSize: 14
          text: "\u2190 Albums"
          MouseArea {
            id: backArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: if (root.service) root.service.navigate("albums")
          }
        }
        Text {
          Layout.fillWidth: true
          elide: Text.ElideRight
          color: Ink.title(Color.foreground, Color.background)
          font.pixelSize: 20
          font.bold: true
          text: root.selected ? String(root.selected.name || "Unknown album") : ""
        }
        Text {
          Layout.fillWidth: true
          elide: Text.ElideRight
          color: Ink.meta(Color.foreground, Color.background)
          font.pixelSize: 12
          text: {
            if (!root.selected) return ""
            var artist = root.selected.album_artist ? String(root.selected.album_artist) : ""
            var year = root.selected.year > 0 ? String(root.selected.year) : ""
            var count = root.loading ? "Loading\u2026" : root.tracks.length + " tracks"
            return [artist, year, count].filter(function(part) { return !!part }).join(" \u00B7 ")
          }
        }
      }

      PillButton {
        Layout.alignment: Qt.AlignVCenter
        label: "\u25B6  Play album"
        enabled: !!(root.service && root.selected)
        onClicked: if (root.service && root.selected)
          root.service.playAlbum(Number(root.selected.id), null)
      }

      ViewToggle {
        Layout.alignment: Qt.AlignVCenter
        service: root.service
      }
    }

    Text {
      visible: root.errorText !== ""
      color: Color.urgent
      font.pixelSize: 12
      text: root.errorText
    }

    EntityList {
      id: albumList
      Layout.fillWidth: true
      Layout.fillHeight: true
      visible: root.selected === null
      items: root.albums
      service: root.service
      covers: true
      playable: true
      emptyText: root.loading ? "" : "No albums yet. Add a music folder in Settings."
      secondaryOf: function(item) {
        return [item.album_artist, item.year > 0 ? String(item.year) : "", item.track_count + " tracks"]
          .filter(function(part) { return !!part }).join(" \u00B7 ")
      }
      onActivated: function(item) { if (root.service) root.service.openAlbum(item) }
      onPlayRequested: function(item) { if (root.service) root.service.playAlbum(Number(item.id), null) }
    }

    TrackList {
      id: trackList
      Layout.fillWidth: true
      Layout.fillHeight: true
      visible: root.selected !== null
      service: root.service
      tracks: root.tracks
      queueScope: "album"
      revealPage: "albums"
      emptyText: root.loading ? "" : "No tracks"
    }
  }
}

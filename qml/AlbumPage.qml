pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import qs.Commons

// Albums list (query kind "albums"); selecting one shows its tracks
// (kind "album", q = album id) with a Play album button (play_album RPC).
Item {
  id: root

  property var service: null

  property var albums: []
  property var tracks: []
  property bool loading: false
  property string errorText: ""
  readonly property var selected: service ? service.selectedAlbum : null
  readonly property int revision: service ? service.libraryRevision : 0
  readonly property bool connected: service ? service.connected : false

  function refresh() {
    if (!service || !service.connected) {
      albums = []
      tracks = []
      errorText = "endbossd is not running"
      return
    }
    loading = true
    if (selected) {
      service.query("album", String(selected.id), 500, 0, function(ok, result, error) {
        root.loading = false
        root.errorText = ok ? "" : error
        if (ok) root.tracks = Array.isArray(result) ? result : []
      })
    } else {
      service.query("albums", "", 500, 0, function(ok, result, error) {
        root.loading = false
        root.errorText = ok ? "" : error
        if (ok) root.albums = Array.isArray(result) ? result : []
      })
    }
  }

  onSelectedChanged: {
    tracks = []
    refresh()
  }
  onRevisionChanged: refresh()
  onConnectedChanged: refresh()
  Component.onCompleted: refresh()

  ColumnLayout {
    anchors.fill: parent
    anchors.margins: 20
    spacing: 10

    RowLayout {
      Layout.fillWidth: true
      spacing: 12

      Text {
        visible: root.selected !== null
        color: backArea.containsMouse ? Color.accent : Color.muted
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
        color: Color.foreground
        font.pixelSize: 20
        font.bold: true
        text: root.selected ? String(root.selected.name || "Unknown album") : "Albums"
      }
      Text {
        color: Color.muted
        font.pixelSize: 12
        text: root.loading ? "Loading\u2026"
          : (root.selected ? root.tracks.length + " tracks" : root.albums.length + " albums")
      }
      Rectangle {
        visible: root.selected !== null
        implicitWidth: playLabel.implicitWidth + 24
        implicitHeight: 30
        radius: 15
        color: Color.accent
        Text {
          id: playLabel
          anchors.centerIn: parent
          color: Color.background
          font.pixelSize: 12
          font.bold: true
          text: "\u25B6 Play album"
        }
        MouseArea {
          anchors.fill: parent
          cursorShape: Qt.PointingHandCursor
          onClicked: if (root.service && root.selected)
            root.service.playAlbum(Number(root.selected.id), null)
        }
      }
    }

    Text {
      visible: root.errorText !== ""
      color: Color.urgent
      font.pixelSize: 12
      text: root.errorText
    }

    EntityList {
      Layout.fillWidth: true
      Layout.fillHeight: true
      visible: root.selected === null
      items: root.albums
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
      Layout.fillWidth: true
      Layout.fillHeight: true
      visible: root.selected !== null
      service: root.service
      tracks: root.tracks
      emptyText: root.loading ? "" : "No tracks"
    }
  }
}

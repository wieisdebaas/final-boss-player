pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import qs.Commons

// Landing page: library stats, a few albums and tracks (query kind "home").
Item {
  id: root

  property var service: null

  property var stats: null
  property var albums: []
  property var tracks: []
  property bool loading: false
  property string errorText: ""
  readonly property int sectionLimit: 8
  readonly property int revision: service ? service.libraryRevision : 0
  readonly property bool connected: service ? service.connected : false

  function refresh() {
    if (!service || !service.connected) {
      stats = null
      albums = []
      tracks = []
      errorText = "finalbossd is not running"
      return
    }
    loading = true
    service.query("home", "", sectionLimit, 0, function(ok, result, error) {
      root.loading = false
      if (!ok || !result) {
        root.errorText = ok ? "Empty response" : error
        return
      }
      root.errorText = ""
      root.stats = result.stats || null
      root.albums = Array.isArray(result.albums) ? result.albums : []
      root.tracks = Array.isArray(result.tracks) ? result.tracks : []
    })
  }

  onRevisionChanged: refresh()
  onConnectedChanged: refresh()
  Component.onCompleted: refresh()

  ColumnLayout {
    anchors.fill: parent
    anchors.margins: 20
    spacing: 8

    RowLayout {
      Layout.fillWidth: true
      Text {
        Layout.fillWidth: true
        elide: Text.ElideRight
        color: Color.foreground
        font.pixelSize: 20
        font.bold: true
        text: "Home"
      }
      ViewToggle {
        Layout.alignment: Qt.AlignVCenter
        service: root.service
      }
    }
    Text {
      color: root.errorText !== "" ? Color.urgent : Color.muted
      font.pixelSize: 12
      text: root.errorText !== "" ? root.errorText
        : (root.loading ? "Loading\u2026"
          : (root.stats
            ? root.stats.tracks + " tracks \u00B7 " + root.stats.albums + " albums \u00B7 "
              + root.stats.artists + " artists"
            : ""))
    }

    Text {
      Layout.topMargin: 8
      color: Color.foreground
      font.pixelSize: 14
      font.bold: true
      text: "Albums"
    }
    EntityList {
      Layout.fillWidth: true
      Layout.fillHeight: true
      items: root.albums
      service: root.service
      covers: true
      playable: true
      emptyText: "No albums yet. Add a music folder in Settings."
      secondaryOf: function(item) {
        return [item.album_artist, item.track_count + " tracks"]
          .filter(function(part) { return !!part }).join(" \u00B7 ")
      }
      onActivated: function(item) { if (root.service) root.service.openAlbum(item) }
      onPlayRequested: function(item) { if (root.service) root.service.playAlbum(Number(item.id), null) }
    }

    Text {
      Layout.topMargin: 8
      color: Color.foreground
      font.pixelSize: 14
      font.bold: true
      text: "Tracks"
    }
    TrackList {
      Layout.fillWidth: true
      Layout.fillHeight: true
      service: root.service
      tracks: root.tracks
      showNumbers: false
      emptyText: "No tracks yet"
    }
  }
}

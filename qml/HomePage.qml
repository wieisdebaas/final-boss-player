pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import qs.Commons

// Landing page: library stats and the most recently added tracks.
Item {
  id: root

  property var service: null

  property var stats: null
  property var tracks: []
  property bool loading: false
  property string errorText: ""
  readonly property int recentLimit: 100
  readonly property var keyboardLists: [trackList]
  readonly property int revision: service ? service.libraryRevision : 0
  readonly property bool connected: service ? service.connected : false

  function refresh() {
    if (!service || !service.connected) {
      stats = null
      tracks = []
      errorText = "finalbossd is not running"
      return
    }
    loading = true
    service.query("home", "", recentLimit, 0, function(ok, result, error) {
      root.loading = false
      if (!ok || !result) {
        root.errorText = ok ? "Empty response" : error
        return
      }
      root.errorText = ""
      root.stats = result.stats || null
      root.tracks = Array.isArray(result.tracks) ? result.tracks : []
    })
  }

  onRevisionChanged: refresh()
  onConnectedChanged: refresh()
  Component.onCompleted: refresh()

  RowLayout {
    id: header
    anchors.top: parent.top
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.margins: 20
    spacing: 12

    ColumnLayout {
      Layout.fillWidth: true
      spacing: 2

      Text {
        Layout.fillWidth: true
        elide: Text.ElideRight
        color: Color.foreground
        font.pixelSize: 20
        font.bold: true
        text: "Just added"
      }

      Text {
        Layout.fillWidth: true
        elide: Text.ElideRight
        color: root.errorText !== "" ? Color.urgent : Color.muted
        font.pixelSize: 12
        text: root.errorText !== "" ? root.errorText
          : (root.loading ? "Loading\u2026"
            : (root.stats
              ? root.stats.tracks + " tracks \u00B7 " + root.stats.albums + " albums \u00B7 "
                + root.stats.artists + " artists"
              : ""))
      }
    }

    ViewToggle {
      Layout.alignment: Qt.AlignVCenter
      service: root.service
    }
  }

  TrackList {
    id: trackList
    anchors.top: header.bottom
    anchors.topMargin: 12
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    anchors.leftMargin: 12
    anchors.rightMargin: 4
    service: root.service
    tracks: root.tracks
    showNumbers: false
    covers: true
    emptyText: root.errorText !== "" ? root.errorText : "No tracks yet. Add a music folder in Settings."
  }
}

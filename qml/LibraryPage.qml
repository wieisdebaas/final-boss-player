pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import qs.Commons
import "ink.js" as Ink

// Flat track list from the daemon's `query` RPC (kind "tracks").
Item {
  id: root

  property var service: null

  property var tracks: []
  property bool loading: false
  property string errorText: ""
  property int loadSerial: 0
  readonly property int revision: service ? service.libraryRevision : 0
  readonly property bool connected: service ? service.connected : false
  readonly property var keyboardLists: [list]

  function refresh() {
    root.loadSerial += 1
    var serial = root.loadSerial
    if (!service || !service.connected) {
      tracks = []
      errorText = "finalbossd is not running"
      return
    }
    loading = true
    service.queryPages("tracks", "", function(ok, result, error) {
      if (serial !== root.loadSerial) return
      root.loading = false
      root.errorText = ok ? "" : error
      if (ok) root.tracks = result
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

    Text {
      Layout.fillWidth: true
      elide: Text.ElideRight
      color: Ink.title(Color.foreground, Color.background)
      font.pixelSize: 20
      font.bold: true
      text: "Library"
    }

    Text {
      color: Ink.meta(Color.foreground, Color.background)
      font.pixelSize: 12
      text: root.loading ? "Loading\u2026" : root.tracks.length + " tracks"
    }

    ViewToggle {
      Layout.alignment: Qt.AlignVCenter
      service: root.service
    }
  }

  TrackList {
    id: list
    anchors.top: header.bottom
    anchors.topMargin: 12
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    anchors.leftMargin: 12
    anchors.rightMargin: 4
    service: root.service
    tracks: root.tracks
    queueScope: "library"
    revealPage: "library"
    covers: true
    emptyText: root.errorText !== "" ? root.errorText : "No tracks yet"
  }

  Text {
    anchors.horizontalCenter: list.horizontalCenter
    anchors.top: list.verticalCenter
    anchors.topMargin: 24
    visible: !root.loading && root.tracks.length === 0 && root.errorText === ""
    color: Ink.quiet(Color.foreground, Color.background)
    font.pixelSize: 12
    text: "Add a music folder in Settings."
  }
}

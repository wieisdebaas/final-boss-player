pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons

// Flat track list from the daemon's `query` RPC (kind "tracks").
Item {
  id: root

  property var service: null

  property var tracks: []
  property bool loading: false
  property string errorText: ""
  property int loadSerial: 0
  // The daemon returns at most 500 tracks per query. Page until a short page
  // so the header count and the list are the whole library.
  readonly property int pageLimit: 500
  readonly property int revision: service ? service.libraryRevision : 0
  readonly property bool connected: service ? service.connected : false

  function refresh() {
    root.loadSerial += 1
    var serial = root.loadSerial
    if (!service || !service.connected) {
      tracks = []
      errorText = "finalbossd is not running"
      return
    }
    loading = true
    var collected = []
    function take(offset) {
      service.query("tracks", "", root.pageLimit, offset, function(ok, result, error) {
        if (serial !== root.loadSerial) return
        if (!ok) {
          root.loading = false
          root.errorText = error
          return
        }
        var batch = Array.isArray(result) ? result : []
        for (var i = 0; i < batch.length; ++i) collected.push(batch[i])
        if (batch.length === root.pageLimit) {
          take(offset + batch.length)
          return
        }
        root.loading = false
        root.errorText = ""
        root.tracks = collected
      })
    }
    take(0)
  }

  onRevisionChanged: refresh()
  onConnectedChanged: refresh()
  Component.onCompleted: refresh()

  Text {
    id: titleText
    anchors.top: parent.top
    anchors.left: parent.left
    anchors.margins: 20
    color: Color.foreground
    font.pixelSize: 20
    font.bold: true
    text: "Library"
  }

  Text {
    anchors.left: titleText.right
    anchors.leftMargin: 12
    anchors.baseline: titleText.baseline
    color: Color.muted
    font.pixelSize: 12
    text: root.loading ? "Loading\u2026" : root.tracks.length + " tracks"
  }

  TrackList {
    id: list
    anchors.top: titleText.bottom
    anchors.topMargin: 12
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    anchors.leftMargin: 12
    anchors.rightMargin: 4
    service: root.service
    tracks: root.tracks
    queueScope: "library"
    emptyText: root.errorText !== "" ? root.errorText : "No tracks yet"
  }

  Text {
    anchors.horizontalCenter: list.horizontalCenter
    anchors.top: list.verticalCenter
    anchors.topMargin: 24
    visible: !root.loading && root.tracks.length === 0 && root.errorText === ""
    color: Color.muted
    font.pixelSize: 12
    text: "Add a music folder in Settings."
  }
}

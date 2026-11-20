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
  readonly property int pageLimit: 500
  readonly property int revision: service ? service.libraryRevision : 0
  readonly property bool connected: service ? service.connected : false
  readonly property int currentId: service && service.track && service.track.id !== undefined
    ? Number(service.track.id) : -1

  function formatDuration(ms) {
    var total = Math.max(0, Math.floor(Number(ms) / 1000))
    var seconds = total % 60
    return Math.floor(total / 60) + ":" + (seconds < 10 ? "0" : "") + seconds
  }

  function refresh() {
    if (!service || !service.connected) {
      tracks = []
      errorText = "endbossd is not running"
      return
    }
    loading = true
    service.query("tracks", "", pageLimit, 0, function(ok, result, error) {
      root.loading = false
      if (!ok) {
        root.errorText = error
        return
      }
      root.errorText = ""
      root.tracks = Array.isArray(result) ? result : []
    })
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

  ListView {
    id: list
    anchors.top: titleText.bottom
    anchors.topMargin: 12
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    anchors.leftMargin: 12
    anchors.rightMargin: 12
    clip: true
    model: root.tracks
    boundsBehavior: Flickable.StopAtBounds

    delegate: Rectangle {
      id: row
      required property var modelData
      required property int index
      width: ListView.view.width
      height: 44
      radius: 6
      color: rowArea.containsMouse
        ? Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.08)
        : "transparent"

      readonly property bool isCurrent: Number(modelData.id) === root.currentId

      Row {
        anchors.fill: parent
        anchors.leftMargin: 10
        anchors.rightMargin: 10
        spacing: 12

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
              : String(row.index + 1)
        }

        Column {
          anchors.verticalCenter: parent.verticalCenter
          width: parent.width - 28 - 60 - 24
          spacing: 1
          Text {
            width: parent.width
            elide: Text.ElideRight
            color: row.isCurrent ? Color.accent : Color.foreground
            font.pixelSize: 13
            text: String(row.modelData.title || "Untitled")
          }
          Text {
            width: parent.width
            elide: Text.ElideRight
            color: Color.muted
            font.pixelSize: 11
            text: [row.modelData.artist, row.modelData.album]
              .filter(function(part) { return !!part }).join(" \u00B7 ")
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

      MouseArea {
        id: rowArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: if (root.service) root.service.playOrToggle(Number(row.modelData.id), null)
      }
    }
  }

  Column {
    anchors.centerIn: list
    spacing: 6
    visible: !root.loading && root.tracks.length === 0
    Text {
      anchors.horizontalCenter: parent.horizontalCenter
      color: Color.foreground
      font.pixelSize: 14
      text: root.errorText !== "" ? root.errorText : "No tracks yet"
    }
    Text {
      anchors.horizontalCenter: parent.horizontalCenter
      color: Color.muted
      font.pixelSize: 12
      visible: root.errorText === ""
      text: "Add a music folder in Settings."
    }
  }
}

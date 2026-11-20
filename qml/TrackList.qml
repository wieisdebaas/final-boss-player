pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons

// Reusable track rows. Clicking a row plays that track via Service.play().
ListView {
  id: root

  property var service: null
  property var tracks: []
  property string emptyText: "No tracks"
  property bool showNumbers: true

  readonly property int currentId: service && service.track && service.track.id !== undefined
    ? Number(service.track.id) : -1

  function formatDuration(ms) {
    var total = Math.max(0, Math.floor(Number(ms) / 1000))
    var seconds = total % 60
    return Math.floor(total / 60) + ":" + (seconds < 10 ? "0" : "") + seconds
  }

  clip: true
  model: tracks
  boundsBehavior: Flickable.StopAtBounds

  delegate: Rectangle {
    id: row
    required property var modelData
    required property int index
    readonly property bool isCurrent: Number(modelData.id) === root.currentId

    width: ListView.view.width
    height: 44
    radius: 6
    color: rowArea.containsMouse
      ? Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.08)
      : "transparent"

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
        visible: root.showNumbers
        text: row.isCurrent && root.service && root.service.playing
          ? "\u25B6"
          : String(row.modelData.track_no > 0 ? row.modelData.track_no : row.index + 1)
      }

      Column {
        anchors.verticalCenter: parent.verticalCenter
        width: parent.width - (root.showNumbers ? 40 : 0) - 60 - 24
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
      onClicked: if (root.service) root.service.play(Number(row.modelData.id), null)
    }
  }

  Text {
    anchors.centerIn: parent
    visible: root.count === 0
    color: Color.muted
    font.pixelSize: 13
    text: root.emptyText
  }
}

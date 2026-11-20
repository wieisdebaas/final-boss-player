pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons

// Bottom transport bar. Pure view over Service.playerState.
Rectangle {
  id: root

  property var service: null

  readonly property var snapshot: service ? service.playerState : null
  readonly property real positionMs: snapshot ? Number(snapshot.position_ms) || 0 : 0
  readonly property real durationMs: snapshot && snapshot.duration_ms ? Number(snapshot.duration_ms) : 0
  readonly property real progress: durationMs > 0
    ? Math.max(0, Math.min(1, positionMs / durationMs)) : 0

  implicitHeight: 72
  color: Qt.darker(Color.background, 1.15)

  function formatTime(ms) {
    var total = Math.max(0, Math.floor(ms / 1000))
    var minutes = Math.floor(total / 60)
    var seconds = total % 60
    return minutes + ":" + (seconds < 10 ? "0" : "") + seconds
  }

  Rectangle {
    anchors.top: parent.top
    anchors.left: parent.left
    anchors.right: parent.right
    height: 1
    color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.12)
  }

  Row {
    anchors.left: parent.left
    anchors.leftMargin: 16
    anchors.verticalCenter: parent.verticalCenter
    width: 240
    Column {
      width: parent.width
      spacing: 2
      Text {
        width: parent.width
        elide: Text.ElideRight
        color: Color.foreground
        font.pixelSize: 14
        font.bold: true
        text: root.service && root.service.hasMedia ? root.service.title : "Nothing playing"
      }
      Text {
        width: parent.width
        elide: Text.ElideRight
        color: Color.muted
        font.pixelSize: 12
        text: root.service ? root.service.artist : ""
      }
    }
  }

  Column {
    anchors.centerIn: parent
    width: Math.min(420, parent.width - 560)
    spacing: 6

    Row {
      anchors.horizontalCenter: parent.horizontalCenter
      spacing: 20
      Repeater {
        model: [
          { glyph: "\u23EE", action: "previous" },
          { glyph: root.service && root.service.playing ? "\u23F8" : "\u25B6", action: "toggle" },
          { glyph: "\u23ED", action: "next" }
        ]
        delegate: Text {
          id: transport
          required property var modelData
          color: Color.foreground
          font.pixelSize: 20
          opacity: root.service && root.service.connected ? 1 : 0.4
          text: modelData.glyph
          MouseArea {
            anchors.fill: parent
            anchors.margins: -6
            cursorShape: Qt.PointingHandCursor
            onClicked: if (root.service) root.service[transport.modelData.action]()
          }
        }
      }
    }

    Row {
      width: parent.width
      spacing: 8
      Text {
        color: Color.muted
        font.pixelSize: 11
        text: root.formatTime(root.positionMs)
      }
      Rectangle {
        id: track
        width: parent.width - 80
        height: 4
        radius: 2
        anchors.verticalCenter: parent.verticalCenter
        color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.18)
        Rectangle {
          width: parent.width * root.progress
          height: parent.height
          radius: 2
          color: Color.accent
        }
        MouseArea {
          anchors.fill: parent
          anchors.topMargin: -8
          anchors.bottomMargin: -8
          enabled: root.durationMs > 0
          cursorShape: Qt.PointingHandCursor
          onClicked: function(mouse) {
            if (root.service) root.service.seek(root.durationMs * mouse.x / track.width)
          }
        }
      }
      Text {
        color: Color.muted
        font.pixelSize: 11
        text: root.formatTime(root.durationMs)
      }
    }
  }

  Row {
    anchors.right: parent.right
    anchors.rightMargin: 16
    anchors.verticalCenter: parent.verticalCenter
    spacing: 8
    Text {
      anchors.verticalCenter: parent.verticalCenter
      color: Color.muted
      font.pixelSize: 12
      text: "vol"
    }
    Rectangle {
      id: volumeTrack
      width: 100
      height: 4
      radius: 2
      anchors.verticalCenter: parent.verticalCenter
      color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.18)
      Rectangle {
        width: parent.width * (root.snapshot ? Math.max(0, Math.min(1, Number(root.snapshot.volume))) : 0)
        height: parent.height
        radius: 2
        color: Color.accent
      }
      MouseArea {
        anchors.fill: parent
        anchors.topMargin: -8
        anchors.bottomMargin: -8
        cursorShape: Qt.PointingHandCursor
        onClicked: function(mouse) {
          if (root.service) root.service.setVolume(mouse.x / volumeTrack.width)
        }
      }
    }
  }
}

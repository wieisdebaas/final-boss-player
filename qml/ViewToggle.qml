pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import qs.Commons

// List / tiles switch. Every page puts this at the right of its header.
// The choice lives on the service, so one click changes every page.
Row {
  id: root

  property var service: null

  spacing: 2

  Repeater {
    model: [
      { tiles: false, listMark: true },
      { tiles: true, listMark: false }
    ]

    delegate: Rectangle {
      id: button
      required property var modelData
      readonly property bool on: !!(root.service && root.service.tiles === button.modelData.tiles)

      width: 28
      height: 28
      radius: 6
      color: button.on
        ? Qt.rgba(Color.accent.r, Color.accent.g, Color.accent.b, 0.22)
        : (area.containsMouse
          ? Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.08)
          : "transparent")

      Item {
        id: mark
        anchors.centerIn: parent
        width: 14
        height: 14
        readonly property color ink: button.on ? Color.accent
          : (area.containsMouse ? Color.foreground : Color.muted)

        Column {
          anchors.centerIn: parent
          spacing: 2
          visible: button.modelData.listMark
          Repeater {
            model: 3
            Rectangle {
              width: 12
              height: 2
              radius: 1
              color: mark.ink
            }
          }
        }

        Grid {
          anchors.centerIn: parent
          columns: 2
          spacing: 2
          visible: !button.modelData.listMark
          Repeater {
            model: 4
            Rectangle {
              width: 5
              height: 5
              radius: 1
              color: mark.ink
            }
          }
        }
      }

      ToolTip.visible: area.containsMouse
      ToolTip.delay: 400
      ToolTip.text: button.modelData.listMark ? "List  Ctrl+L" : "Tiles  Ctrl+T"

      MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: if (root.service) root.service.tiles = button.modelData.tiles
      }
    }
  }
}

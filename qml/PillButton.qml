pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons

// Text action. Filled pills are the accent actions; outlined pills are the rest.
Rectangle {
  id: root

  property string label: ""
  property bool filled: true
  property bool enabled: true
  readonly property int buttonHeight: 34

  signal clicked()

  implicitWidth: labelText.implicitWidth + 28
  width: implicitWidth
  height: buttonHeight
  implicitHeight: buttonHeight
  radius: height / 2
  opacity: enabled ? 1 : 0.4
  border.width: filled ? 0 : 1
  border.color: area.containsMouse ? Qt.lighter(Color.accent, 1.15) : Color.accent
  color: {
    if (root.filled) {
      if (area.pressed) return Qt.darker(Color.accent, 1.25)
      if (area.containsMouse) return Qt.lighter(Color.accent, 1.12)
      return Color.accent
    }
    if (area.pressed)
      return Qt.rgba(Color.accent.r, Color.accent.g, Color.accent.b, 0.22)
    if (area.containsMouse)
      return Qt.rgba(Color.accent.r, Color.accent.g, Color.accent.b, 0.14)
    return "transparent"
  }

  Behavior on color {
    ColorAnimation { duration: 120 }
  }

  Text {
    id: labelText
    anchors.fill: parent
    horizontalAlignment: Text.AlignHCenter
    verticalAlignment: Text.AlignVCenter
    color: root.filled ? Color.background : Color.accent
    font.pixelSize: 13
    font.bold: true
    text: root.label
  }

  MouseArea {
    id: area
    anchors.fill: parent
    hoverEnabled: true
    enabled: root.enabled
    cursorShape: Qt.PointingHandCursor
    onClicked: root.clicked()
  }
}

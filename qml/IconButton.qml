pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons
import "ink.js" as Ink

// Round transport control. `primary` is the filled play disc; `active` is the
// same fill used when shuffle is on.
Item {
  id: root

  property string glyph: ""
  property bool primary: false
  property bool active: false
  property bool enabled: true
  property int diameter: 36
  property int glyphOffset: 0

  signal clicked()

  width: diameter
  height: diameter
  implicitWidth: diameter
  implicitHeight: diameter
  opacity: enabled ? 1 : 0.4

  Rectangle {
    id: disc
    anchors.fill: parent
    radius: width / 2
    color: {
      var filled = root.primary || root.active
      if (filled) {
        if (area.pressed) return Qt.darker(Color.accent, 1.25)
        if (area.containsMouse) return Qt.lighter(Color.accent, 1.12)
        return Color.accent
      }
      if (area.pressed)
        return Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.16)
      if (area.containsMouse)
        return Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.1)
      return Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.06)
    }
    border.width: root.primary || root.active ? 0 : 1
    border.color: area.containsMouse
      ? Color.accent
      : Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.28)

    Behavior on color {
      ColorAnimation { duration: 120 }
    }

    Text {
      anchors.fill: parent
      anchors.leftMargin: root.glyphOffset
      horizontalAlignment: Text.AlignHCenter
      verticalAlignment: Text.AlignVCenter
      color: (root.primary || root.active)
        ? Color.background
        : (area.containsMouse ? Color.accent : Ink.title(Color.foreground, Color.background))
      font.pixelSize: 14
      text: root.glyph
    }
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

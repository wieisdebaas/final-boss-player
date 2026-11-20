import QtQuick
import qs.Commons
import qs.Ui

// Minimal stub: a music glyph that toggles the full panel. The mini-player
// popup (Task 10) will replace this.
BarWidget {
  id: root

  readonly property string pluginId: "io.github.endboss.player"
  readonly property var player: bar && bar.shell ? bar.shell.serviceFor(pluginId) : null

  moduleName: pluginId
  implicitWidth: glyph.implicitWidth + 16
  implicitHeight: barSize

  Text {
    id: glyph
    anchors.centerIn: parent
    color: root.bar ? root.bar.foreground : Color.foreground
    opacity: root.player && root.player.connected ? 1 : 0.5
    font.pixelSize: 14
    text: root.player && root.player.playing ? "\u266B" : "\u266A"
  }

  MouseArea {
    anchors.fill: parent
    cursorShape: Qt.PointingHandCursor
    onClicked: if (root.bar && root.bar.shell) root.bar.shell.toggle(root.pluginId, "{}")
  }
}

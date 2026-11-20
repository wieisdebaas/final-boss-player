pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import qs.Commons

import "qml"

// Full player window: sidebar (Library, Settings) + page stack + PlayerBar.
// All state lives in Service.qml; the host injects `service`.
Item {
  id: root

  property var shell: null
  property var manifest: null
  property var service: null
  property bool opened: false
  property bool closingFromHost: false

  readonly property string pluginId: manifest && manifest.id
    ? String(manifest.id) : "io.github.endboss.player"
  readonly property string currentPage: service ? service.currentPage : "library"

  readonly property var sidebarItems: [
    { page: "library", label: "Library" },
    { page: "settings", label: "Settings" }
  ]

  // Host contract: open(payloadJson) / close().
  function open(payloadJson) {
    var payload = ({})
    try { payload = JSON.parse(String(payloadJson || "{}")) || ({}) } catch (e) {}
    if (service) {
      if (payload.page) service.navigate(String(payload.page))
      service.setUiVisible(true)
    }
    closingFromHost = false
    opened = true
    Qt.callLater(function() { focusScope.forceActiveFocus() })
  }

  function close() {
    closingFromHost = true
    opened = false
    if (service) service.setUiVisible(false)
    closingFromHost = false
  }

  function requestClose() {
    if (shell && typeof shell.hide === "function") shell.hide(pluginId)
    else close()
  }

  FloatingWindow {
    id: window
    visible: root.opened
    title: "Endboss Player"
    color: Color.background
    implicitWidth: 900
    implicitHeight: 640
    minimumSize: Qt.size(640, 440)

    onVisibleChanged: {
      if (!visible && root.opened && !root.closingFromHost) root.requestClose()
    }

    FocusScope {
      id: focusScope
      anchors.fill: parent
      focus: true
      Keys.onEscapePressed: root.requestClose()

      Row {
        id: body
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: playerBar.top

        Rectangle {
          id: sidebar
          width: 180
          height: parent.height
          color: Qt.darker(Color.background, 1.08)

          Column {
            anchors.fill: parent
            anchors.margins: 12
            spacing: 4

            Text {
              color: Color.accent
              font.pixelSize: 15
              font.bold: true
              bottomPadding: 12
              text: "Endboss"
            }

            Repeater {
              model: root.sidebarItems
              delegate: Rectangle {
                id: item
                required property var modelData
                readonly property bool selected: root.currentPage === modelData.page
                width: parent.width
                height: 36
                radius: 6
                color: selected
                  ? Qt.rgba(Color.accent.r, Color.accent.g, Color.accent.b, 0.18)
                  : (itemArea.containsMouse
                    ? Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.08)
                    : "transparent")
                Text {
                  anchors.verticalCenter: parent.verticalCenter
                  anchors.left: parent.left
                  anchors.leftMargin: 12
                  color: item.selected ? Color.accent : Color.foreground
                  font.pixelSize: 13
                  text: item.modelData.label
                }
                MouseArea {
                  id: itemArea
                  anchors.fill: parent
                  hoverEnabled: true
                  cursorShape: Qt.PointingHandCursor
                  onClicked: if (root.service) root.service.navigate(item.modelData.page)
                }
              }
            }
          }
        }

        Item {
          width: parent.width - sidebar.width
          height: parent.height

          LibraryPage {
            anchors.fill: parent
            visible: root.currentPage === "library"
            service: root.service
          }
          SettingsPage {
            anchors.fill: parent
            visible: root.currentPage === "settings"
            service: root.service
          }
        }
      }

      PlayerBar {
        id: playerBar
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        service: root.service
      }
    }
  }
}

pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import qs.Commons
import qs.Ui

import "qml"

// Full player window: sidebar + page stack + PlayerBar + keyboard shortcuts.
// All state lives in Service.qml; the host injects `service`.
Item {
  id: root

  property var shell: null
  property var manifest: null
  property var service: null
  property bool opened: false
  property bool closingFromHost: false

  readonly property string pluginId: manifest && manifest.id
    ? String(manifest.id) : "io.github.finalboss.player"
  readonly property string currentPage: service ? service.currentPage : "home"

  readonly property var sidebarItems: [
    { page: "home", label: "Home" },
    { page: "search", label: "Search" },
    { page: "library", label: "Library" },
    { page: "artists", label: "Artists" },
    { page: "albums", label: "Albums" },
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
    title: "Final Boss Player"
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

      // Return keyboard focus to the scope when leaving the search field.
      Connections {
        target: root
        function onCurrentPageChanged() {
          if (root.currentPage !== "search") focusScope.forceActiveFocus()
        }
      }

      // Window shortcuts receive ShortcutOverride events before focused text
      // inputs can consume editing keys.
      Shortcut {
        sequence: "Space"
        context: Qt.WindowShortcut
        onActivated: if (root.service) root.service.toggle()
      }
      Shortcut {
        sequence: "Ctrl+Left"
        context: Qt.WindowShortcut
        onActivated: if (root.service) root.service.previous()
      }
      Shortcut {
        sequence: "Ctrl+Right"
        context: Qt.WindowShortcut
        onActivated: if (root.service) root.service.next()
      }
      Shortcut {
        sequence: "Shift+Left"
        context: Qt.WindowShortcut
        onActivated: if (root.service) root.service.seekBy(-5000)
      }
      Shortcut {
        sequence: "Shift+Right"
        context: Qt.WindowShortcut
        onActivated: if (root.service) root.service.seekBy(5000)
      }
      Shortcut {
        sequence: "Ctrl+Up"
        context: Qt.WindowShortcut
        onActivated: if (root.service) root.service.volumeBy(0.05)
      }
      Shortcut {
        sequence: "Ctrl+Down"
        context: Qt.WindowShortcut
        onActivated: if (root.service) root.service.volumeBy(-0.05)
      }
      Shortcut {
        sequence: "M"
        context: Qt.WindowShortcut
        onActivated: if (root.service) root.service.toggleMute()
      }
      Shortcut {
        sequence: "Ctrl+F"
        context: Qt.WindowShortcut
        onActivated: if (root.service) root.service.focusSearch()
      }
      Shortcut {
        sequence: "Escape"
        context: Qt.WindowShortcut
        onActivated: root.requestClose()
      }

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

            Column {
              width: parent.width
              spacing: 8
              bottomPadding: 14

              SizedImage {
                width: parent.width
                height: width
                url: Qt.resolvedUrl("assets/logo.jpg")
              }

              Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
                color: Color.accent
                font.pixelSize: 20
                font.bold: true
                text: "Final Boss Player"
              }
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

          HomePage {
            anchors.fill: parent
            visible: root.currentPage === "home"
            service: root.service
          }
          SearchPage {
            anchors.fill: parent
            visible: root.currentPage === "search"
            service: root.service
          }
          LibraryPage {
            anchors.fill: parent
            visible: root.currentPage === "library"
            service: root.service
          }
          ArtistPage {
            anchors.fill: parent
            visible: root.currentPage === "artists"
            service: root.service
          }
          AlbumPage {
            anchors.fill: parent
            visible: root.currentPage === "albums"
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

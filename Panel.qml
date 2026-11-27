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
  // "menu" highlights the sidebar. "list" highlights the page list.
  property string keyRegion: "list"
  property int menuIndex: 0
  property int contentSlot: 0
  property var focusedList: null

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
    Qt.callLater(function() {
      root.keyRegion = "list"
      focusScope.forceActiveFocus()
      root.ensureList()
    })
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

  function clampMenu(index) {
    var count = sidebarItems.length
    if (count <= 0) return 0
    if (index < 0) return 0
    if (index >= count) return count - 1
    return index
  }

  function menuPageCount() {
    return Math.max(1, Math.floor(sidebar.height / 40))
  }

  function pageLists() {
    var page = null
    if (currentPage === "home") page = homePage
    else if (currentPage === "search") page = searchPage
    else if (currentPage === "library") page = libraryPage
    else if (currentPage === "artists") page = artistPage
    else if (currentPage === "albums") page = albumPage
    if (!page || !page.keyboardLists) return []
    var raw = page.keyboardLists
    var out = []
    for (var i = 0; i < raw.length; ++i) {
      var list = raw[i]
      if (list && list.visible && list.keyboardCount > 0) out.push(list)
    }
    return out
  }

  function setFocusedList(list) {
    if (focusedList && focusedList !== list) focusedList.keyboardFocused = false
    focusedList = list
    if (list) list.keyboardFocused = true
  }

  function ensureList() {
    if (keyRegion !== "list") {
      setFocusedList(null)
      return
    }
    var lists = pageLists()
    var slot = -1
    for (var i = 0; i < lists.length; ++i) {
      if (lists[i] === focusedList) slot = i
    }
    if (slot >= 0) {
      contentSlot = slot
      if (focusedList.keyboardIndex < 0) focusedList.jumpKeyboardStart()
      setFocusedList(focusedList)
      return
    }
    contentSlot = 0
    if (lists.length === 0) {
      setFocusedList(null)
      return
    }
    lists[0].jumpKeyboardStart()
    setFocusedList(lists[0])
  }

  function focusRegion(region) {
    keyRegion = region
    focusScope.forceActiveFocus()
    ensureList()
  }

  // Tab leaves a text field for the list. Shift+Tab leaves it for the sidebar.
  // Otherwise Tab and Shift+Tab switch between those two.
  function moveRegion(toList) {
    if (service && service.textEditing) focusRegion(toList ? "list" : "menu")
    else focusRegion(keyRegion === "menu" ? "list" : "menu")
  }

  function moveInLists(dx, dy) {
    var lists = pageLists()
    if (lists.length === 0) {
      if (currentPage === "settings" && dy !== 0) settingsPage.scrollBy(dy * 48)
      return
    }
    if (contentSlot < 0 || contentSlot >= lists.length) contentSlot = 0
    var list = lists[contentSlot]
    if (dx !== 0 && !list.tiled) return
    var edge = list.moveKeyboard(dx, dy)
    if (dy < 0 && edge === "start" && contentSlot > 0) {
      contentSlot -= 1
      lists[contentSlot].jumpKeyboardEnd()
      setFocusedList(lists[contentSlot])
      return
    }
    if (dy > 0 && edge === "end" && contentSlot + 1 < lists.length) {
      contentSlot += 1
      lists[contentSlot].jumpKeyboardStart()
      setFocusedList(lists[contentSlot])
      return
    }
    setFocusedList(list)
  }

  function movePage(direction) {
    if (keyRegion === "menu") {
      menuIndex = clampMenu(menuIndex + direction * menuPageCount())
      return
    }
    var lists = pageLists()
    if (lists.length === 0) {
      if (currentPage === "settings") settingsPage.scrollBy(direction * settingsPage.scrollPage)
      return
    }
    if (contentSlot < 0 || contentSlot >= lists.length) contentSlot = 0
    lists[contentSlot].pageKeyboard(direction)
    setFocusedList(lists[contentSlot])
  }

  function jumpEdge(toStart) {
    if (keyRegion === "menu") {
      menuIndex = toStart ? 0 : Math.max(0, sidebarItems.length - 1)
      return
    }
    var lists = pageLists()
    if (lists.length === 0) {
      if (currentPage === "settings") settingsPage.scrollTo(toStart)
      return
    }
    if (contentSlot < 0 || contentSlot >= lists.length) contentSlot = 0
    if (toStart) lists[contentSlot].jumpKeyboardStart()
    else lists[contentSlot].jumpKeyboardEnd()
    setFocusedList(lists[contentSlot])
  }

  function pointInItem(item, area, x, y) {
    if (!item) return false
    var local = item.mapFromItem(area, x, y)
    return item.contains(local)
  }

  function editingInput() {
    if (currentPage === "search") return searchPage.editingInput()
    if (currentPage === "settings") return settingsPage.editingInput()
    return null
  }

  // A text field keeps active focus until another item takes it. Clicks on
  // the rest of the window do not, so shortcuts stay swallowed. Drop it.
  function releaseEditing() {
    var field = editingInput()
    if (field && field.focus) field.focus = false
    focusScope.forceActiveFocus()
  }

  // Open the list this queue came from and center the playing track.
  function hopToPlaying() {
    if (!service || !service.openPlayingContext()) return
    keyRegion = "list"
    focusScope.forceActiveFocus()
    ensureList()
  }

  function activateKey() {
    if (keyRegion === "menu") {
      if (service && menuIndex >= 0 && menuIndex < sidebarItems.length)
        service.navigate(sidebarItems[menuIndex].page)
      return
    }
    var lists = pageLists()
    if (contentSlot < 0 || contentSlot >= lists.length) return
    lists[contentSlot].activateKeyboard()
  }

  function handleKey(event) {
    if (service && service.textEditing) return
    var key = event.key
    var ctrl = (event.modifiers & Qt.ControlModifier) !== 0
    if (key === Qt.Key_Up) {
      if (keyRegion === "menu") menuIndex = clampMenu(menuIndex - 1)
      else moveInLists(0, -1)
      event.accepted = true
      return
    }
    if (key === Qt.Key_Down) {
      if (keyRegion === "menu") menuIndex = clampMenu(menuIndex + 1)
      else moveInLists(0, 1)
      event.accepted = true
      return
    }
    if (key === Qt.Key_Left) {
      if (ctrl) {
        if (service) service.previous()
      } else if (keyRegion === "list") {
        moveInLists(-1, 0)
      }
      event.accepted = true
      return
    }
    if (key === Qt.Key_Right) {
      if (ctrl) {
        if (service) service.next()
      } else if (keyRegion === "list") {
        moveInLists(1, 0)
      }
      event.accepted = true
      return
    }
    if (key === Qt.Key_PageUp) { movePage(-1); event.accepted = true; return }
    if (key === Qt.Key_PageDown) { movePage(1); event.accepted = true; return }
    if (ctrl && key === Qt.Key_Home) { jumpEdge(true); event.accepted = true; return }
    if (ctrl && key === Qt.Key_End) { jumpEdge(false); event.accepted = true; return }
    if (key === Qt.Key_Return || key === Qt.Key_Enter) {
      activateKey()
      event.accepted = true
    }
  }

  onCurrentPageChanged: Qt.callLater(function() { root.ensureList() })
  Component.onCompleted: Qt.callLater(function() { root.ensureList() })

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

      Connections {
        target: root.service
        function onKeyboardLayoutChanged() {
          Qt.callLater(function() { root.ensureList() })
        }
      }

      Keys.onPressed: function(event) { root.handleKey(event) }

      // Window shortcuts receive ShortcutOverride events before focused text
      // inputs can consume editing keys.
      Shortcut {
        sequence: "Space"
        context: Qt.WindowShortcut
        onActivated: if (root.service) root.service.toggle()
      }
      Shortcut {
        sequence: "Ctrl+P"
        context: Qt.WindowShortcut
        onActivated: if (root.service) root.service.previous()
      }
      Shortcut {
        sequence: "Ctrl+N"
        context: Qt.WindowShortcut
        onActivated: if (root.service) root.service.next()
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
        sequence: "Ctrl+M"
        context: Qt.WindowShortcut
        onActivated: if (root.service) root.service.toggleMute()
      }
      Shortcut {
        sequence: "Ctrl+S"
        context: Qt.WindowShortcut
        onActivated: if (root.service) root.service.toggleShuffle()
      }
      Shortcut {
        sequence: "Ctrl+F"
        context: Qt.WindowShortcut
        onActivated: if (root.service) root.service.focusSearch()
      }
      // Copy stays available while a text field is focused.
      Shortcut {
        sequence: "Ctrl+C"
        context: Qt.WindowShortcut
        enabled: !(root.service && root.service.textEditing)
        onActivated: root.hopToPlaying()
      }
      Shortcut {
        sequence: "Escape"
        context: Qt.WindowShortcut
        onActivated: root.requestClose()
      }
      Shortcut {
        sequence: "Tab"
        context: Qt.WindowShortcut
        onActivated: root.moveRegion(true)
      }
      Shortcut {
        sequence: "Shift+Tab"
        context: Qt.WindowShortcut
        onActivated: root.moveRegion(false)
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
                required property int index
                readonly property bool selected: root.currentPage === modelData.page
                readonly property bool keyHere: root.keyRegion === "menu" && index === root.menuIndex
                width: parent.width
                height: 36
                radius: 6
                border.width: keyHere ? 1 : 0
                border.color: Color.accent
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
            id: homePage
            anchors.fill: parent
            visible: root.currentPage === "home"
            service: root.service
          }
          SearchPage {
            id: searchPage
            anchors.fill: parent
            visible: root.currentPage === "search"
            service: root.service
          }
          LibraryPage {
            id: libraryPage
            anchors.fill: parent
            visible: root.currentPage === "library"
            service: root.service
          }
          ArtistPage {
            id: artistPage
            anchors.fill: parent
            visible: root.currentPage === "artists"
            service: root.service
          }
          AlbumPage {
            id: albumPage
            anchors.fill: parent
            visible: root.currentPage === "albums"
            service: root.service
          }
          SettingsPage {
            id: settingsPage
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
        menuWidth: sidebar.width
        service: root.service
      }

      // Above the pages, but the press is rejected so the control underneath
      // still receives the click.
      MouseArea {
        id: textFocusRelease
        anchors.fill: parent
        z: 100
        enabled: !!(root.service && root.service.textEditing)
        propagateComposedEvents: true
        scrollGestureEnabled: false
        onPressed: function(mouse) {
          var field = root.editingInput()
          var keep = root.pointInItem(field, textFocusRelease, mouse.x, mouse.y)
            || root.pointInItem(field ? field.parent : null, textFocusRelease, mouse.x, mouse.y)
          mouse.accepted = false
          if (!keep) Qt.callLater(root.releaseEditing)
        }
      }
    }
  }
}

import QtQuick
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// Bar entry + mini-player popup. All state comes from Service.qml
// (`player`); this file never talks to the daemon socket directly except
// through the service's request() for the cached album art path.
//
//   left click   toggle the mini-player popup
//   middle click play / pause
//   right click  open the full player panel
//   scroll       previous / next
//   Esc          closes the popup
BarWidget {
  id: root

  readonly property string pluginId: "io.github.endboss.player"
  readonly property var player: bar && bar.shell ? bar.shell.serviceFor(pluginId) : null
  readonly property bool connected: !!player && player.connected
  readonly property bool hasMedia: !!player && player.hasMedia
  readonly property bool isPlaying: !!player && player.playing
  readonly property string title: player ? player.title : ""
  readonly property string artist: player ? player.artist : ""
  readonly property color foreground: root.bar ? root.bar.foreground : Commons.Color.foreground

  property bool popupOpen: false
  property string artPath: ""
  property int artAlbumId: -1

  readonly property var trackInfo: player ? player.track : null
  readonly property int albumId: trackInfo && trackInfo.album_id !== undefined && trackInfo.album_id !== null
    ? Number(trackInfo.album_id) : -1
  readonly property bool trackHasArt: !!trackInfo && trackInfo.has_art === true && albumId >= 0

  readonly property real positionMs: player && player.playerState
    ? Number(player.playerState.position_ms) || 0 : 0
  readonly property real durationMs: player && player.playerState && player.playerState.duration_ms
    ? Number(player.playerState.duration_ms) : 0
  readonly property real progress: durationMs > 0
    ? Math.max(0, Math.min(1, positionMs / durationMs)) : 0

  moduleName: pluginId
  implicitWidth: row.implicitWidth + Style.space(14)
  implicitHeight: barSize

  function close() { popupOpen = false }
  function open() { popupOpen = true }
  function togglePopup() { popupOpen = !popupOpen }

  function openPanel() {
    popupOpen = false
    if (root.bar && root.bar.shell) root.bar.shell.toggle(root.pluginId, "{}")
  }

  // Fetch the cached cover for the current album once per album change.
  function refreshArt() {
    if (!trackHasArt || !player || !connected) {
      artPath = ""
      artAlbumId = -1
      return
    }
    if (albumId === artAlbumId) return
    var requested = albumId
    artAlbumId = requested
    artPath = ""
    player.request("get_art", { album_id: requested }, function(ok, result) {
      if (requested !== root.artAlbumId) return
      root.artPath = ok && result && result.path ? String(result.path) : ""
    })
  }

  onAlbumIdChanged: refreshArt()
  onTrackHasArtChanged: refreshArt()
  onConnectedChanged: {
    if (!connected) { artPath = ""; artAlbumId = -1 }
    else refreshArt()
  }
  onPopupOpenChanged: if (popupOpen) refreshArt()
  Component.onCompleted: refreshArt()

  Row {
    id: row
    anchors.centerIn: parent
    spacing: Style.space(6)

    Text {
      id: glyph
      anchors.verticalCenter: parent.verticalCenter
      textFormat: Text.PlainText
      color: root.foreground
      opacity: root.connected ? 1 : 0.5
      font.family: root.bar ? root.bar.fontFamily : ""
      font.pixelSize: Style.font.body
      text: root.isPlaying ? "\u266B" : "\u266A"
    }

    Text {
      anchors.verticalCenter: parent.verticalCenter
      visible: !root.vertical && root.hasMedia && root.title !== ""
      width: visible ? Math.min(implicitWidth, Style.space(160)) : 0
      elide: Text.ElideRight
      textFormat: Text.PlainText
      color: root.foreground
      opacity: root.isPlaying ? 1 : 0.7
      font.family: root.bar ? root.bar.fontFamily : ""
      font.pixelSize: Style.font.body
      text: root.title
    }
  }

  MouseArea {
    anchors.fill: parent
    cursorShape: Qt.PointingHandCursor
    acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
    onClicked: function(mouse) {
      if (mouse.button === Qt.MiddleButton) {
        if (root.player) root.player.toggle()
      } else if (mouse.button === Qt.RightButton) {
        root.openPanel()
      } else {
        root.togglePopup()
      }
    }
    onWheel: function(wheel) {
      if (!root.player) return
      if (wheel.angleDelta.y > 0) root.player.previous()
      else if (wheel.angleDelta.y < 0) root.player.next()
    }
  }

  PopupCard {
    id: popup
    anchorItem: root
    bar: root.bar
    owner: root
    open: root.popupOpen
    contentWidth: popup.fittedContentWidth(Style.space(320))
    contentHeight: popup.fittedContentHeight(column.implicitHeight)

    FocusScope {
      id: keys
      anchors.fill: parent
      focus: true
      Keys.onEscapePressed: root.close()

      Connections {
        target: root
        function onPopupOpenChanged() {
          if (root.popupOpen) Qt.callLater(function() { keys.forceActiveFocus() })
        }
      }

      Column {
        id: column
        anchors.left: parent.left
        anchors.right: parent.right
        spacing: Style.space(10)

        Row {
          spacing: Style.space(10)
          width: parent.width

          BorderSurface {
            width: Style.space(64)
            height: Style.space(64)
            radius: Style.spacing.labelGap
            color: Style.normalFillFor(root.foreground, Commons.Color.accent)
            borderSpec: Border.controlSpec("normal", root.foreground, Commons.Color.accent)

            SizedImage {
              anchors.fill: parent
              anchors.margins: Style.space(2)
              crop: true
              url: root.artPath !== "" ? "file://" + root.artPath : ""
              visible: url != ""
            }

            Text {
              anchors.centerIn: parent
              visible: root.artPath === ""
              text: "\u266A"
              color: root.foreground
              font.family: root.bar ? root.bar.fontFamily : ""
              font.pixelSize: Style.font.displayLarge
            }
          }

          Column {
            spacing: Style.space(4)
            width: parent.width - Style.space(74)

            Text {
              width: parent.width
              textFormat: Text.PlainText
              elide: Text.ElideRight
              color: root.foreground
              font.family: root.bar ? root.bar.fontFamily : ""
              font.pixelSize: Style.font.subtitle
              font.bold: true
              text: !root.connected ? "endbossd not running"
                : (root.hasMedia ? root.title : "Nothing playing")
            }

            Text {
              width: parent.width
              visible: text !== ""
              textFormat: Text.PlainText
              elide: Text.ElideRight
              color: Qt.darker(root.foreground, 1.3)
              font.family: root.bar ? root.bar.fontFamily : ""
              font.pixelSize: Style.font.bodySmall
              text: root.artist
            }
          }
        }

        Rectangle {
          width: parent.width
          height: 3
          radius: 1.5
          color: Qt.rgba(root.foreground.r, root.foreground.g, root.foreground.b, 0.18)
          visible: root.durationMs > 0
          Rectangle {
            width: parent.width * root.progress
            height: parent.height
            radius: parent.radius
            color: Commons.Color.accent
          }
        }

        Row {
          anchors.horizontalCenter: parent.horizontalCenter
          spacing: Style.space(6)
          // Transport glyphs keep previous / play / next order in RTL too.
          LayoutMirroring.enabled: false
          LayoutMirroring.childrenInherit: true

          Button {
            iconText: "󰒮"
            foreground: root.foreground
            horizontalPadding: Style.spacing.controlPaddingX
            verticalPadding: Style.spacing.controlPaddingY
            enabled: root.connected
            opacity: enabled ? 1.0 : 0.4
            onClicked: if (root.player) root.player.previous()
          }

          Button {
            iconText: root.isPlaying ? "󰏤" : "󰐊"
            foreground: root.foreground
            horizontalPadding: Style.spacing.panelGap
            verticalPadding: Style.spacing.controlPaddingY
            iconSize: Style.font.iconLarge
            enabled: root.connected
            opacity: enabled ? 1.0 : 0.4
            onClicked: if (root.player) root.player.toggle()
          }

          Button {
            iconText: "󰒭"
            foreground: root.foreground
            horizontalPadding: Style.spacing.controlPaddingX
            verticalPadding: Style.spacing.controlPaddingY
            enabled: root.connected
            opacity: enabled ? 1.0 : 0.4
            onClicked: if (root.player) root.player.next()
          }
        }

        Text {
          anchors.horizontalCenter: parent.horizontalCenter
          color: Qt.darker(root.foreground, 1.5)
          font.family: root.bar ? root.bar.fontFamily : ""
          font.pixelSize: Style.font.caption
          text: "Open library"
          MouseArea {
            anchors.fill: parent
            anchors.margins: -4
            cursorShape: Qt.PointingHandCursor
            onClicked: root.openPanel()
          }
        }
      }
    }
  }
}

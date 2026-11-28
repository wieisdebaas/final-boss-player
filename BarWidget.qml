import QtQuick
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import "qml/ink.js" as Ink

// Bar entry + mini-player popup. All state comes from Service.qml
// (`player`); this file never talks to the daemon socket directly except
// through the service's request() for the cached album art path.
//
//   hover        title and artist
//   left click   toggle the mini-player popup
//   middle click play / pause
//   right click  open the full player panel
//   scroll       previous / next
//   Esc          closes the popup
BarWidget {
  id: root

  readonly property string pluginId: "io.github.finalboss.player"
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
  readonly property real volume: player && player.playerState
    ? Math.max(0, Math.min(1, Number(player.playerState.volume) || 0)) : 0
  readonly property bool shuffleOn: !!player && player.shuffle === true

  readonly property string hoverLabel: {
    if (!hasMedia) return ""
    if (title !== "" && artist !== "") return title + "\n" + artist
    return title !== "" ? title : artist
  }
  // The bar only shows a tooltip while this stays true. The popup already
  // shows the track, so hovering the slot then stays quiet.
  readonly property bool tooltipHovered: barHover.containsMouse && !popupOpen && hoverLabel !== ""

  moduleName: pluginId
  implicitWidth: row.implicitWidth + Style.space(14)
  implicitHeight: barSize

  function close() { popupOpen = false }
  function open() { popupOpen = true }
  function togglePopup() { popupOpen = !popupOpen }

  function syncTooltip() {
    if (!bar) return
    if (tooltipHovered && bar.showTooltip) bar.showTooltip(root, hoverLabel)
    else if (bar.hideTooltip) bar.hideTooltip(root)
  }

  function openPanel() {
    popupOpen = false
    if (root.bar && root.bar.shell) root.bar.shell.toggle(root.pluginId, "{}")
  }

  function volumeGlyph() {
    if (root.volume <= 0.001) return "󰖁"
    if (root.volume < 0.34) return "󰕿"
    if (root.volume < 0.67) return "󰖀"
    return "󰕾"
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
  onPopupOpenChanged: {
    if (popupOpen) refreshArt()
    syncTooltip()
  }
  onHoverLabelChanged: syncTooltip()
  Component.onCompleted: refreshArt()
  Component.onDestruction: if (bar && bar.hideTooltip) bar.hideTooltip(root)

  Row {
    id: row
    anchors.centerIn: parent
    spacing: Style.space(6)

    OpticalGlyph {
      id: logo
      anchors.verticalCenter: parent.verticalCenter
      width: Style.bar.iconCanvas
      height: width
      implicitWidth: width
      implicitHeight: height
      text: "󰝚"
      fontFamily: root.bar ? root.bar.fontFamily : Style.font.family
      fontSize: Style.bar.iconFont
      paintedSize: Style.bar.iconSize
      color: root.foreground
      opacity: root.connected ? 1 : 0.5
    }
  }

  MouseArea {
    id: barHover
    anchors.fill: parent
    hoverEnabled: true
    cursorShape: Qt.PointingHandCursor
    acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
    onEntered: root.syncTooltip()
    onExited: if (root.bar && root.bar.hideTooltip) root.bar.hideTooltip(root)
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
              color: Ink.meta(root.foreground, Commons.Color.background)
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
              color: Ink.title(root.foreground, Commons.Color.background)
              font.family: root.bar ? root.bar.fontFamily : ""
              font.pixelSize: Style.font.subtitle
              font.bold: true
              text: !root.connected ? "finalbossd not running"
                : (root.hasMedia ? root.title : "Nothing playing")
            }

            Text {
              width: parent.width
              visible: text !== ""
              textFormat: Text.PlainText
              elide: Text.ElideRight
              color: Ink.meta(root.foreground, Commons.Color.background)
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
            iconSize: Style.font.iconLarge
            enabled: root.connected
            opacity: enabled ? 1.0 : 0.4
            onClicked: if (root.player) root.player.previous()
          }

          Button {
            iconText: root.isPlaying ? "󰏤" : "󰐊"
            foreground: root.foreground
            horizontalPadding: Style.spacing.controlPaddingX
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
            iconSize: Style.font.iconLarge
            enabled: root.connected
            opacity: enabled ? 1.0 : 0.4
            onClicked: if (root.player) root.player.next()
          }

          Item {
            width: Style.space(8)
            height: 1
          }

          Button {
            iconText: "󰒝"
            foreground: root.shuffleOn ? Commons.Color.accent : root.foreground
            active: root.shuffleOn
            horizontalPadding: Style.spacing.controlPaddingX
            verticalPadding: Style.spacing.controlPaddingY
            iconSize: Style.font.iconLarge
            enabled: root.connected
            opacity: enabled ? 1.0 : 0.4
            onClicked: if (root.player) root.player.toggleShuffle()
          }
        }

        Item {
          id: volumeRow
          width: parent.width
          height: Math.max(volumeButton.implicitHeight, volumeSlider.implicitHeight)
          opacity: root.connected ? 1 : 0.4

          Item {
            id: volumeButton
            width: Style.space(28)
            height: parent.height
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter

            Text {
              anchors.centerIn: parent
              text: root.volumeGlyph()
              color: volumeHit.containsMouse ? Commons.Color.accent : root.foreground
              font.family: root.bar ? root.bar.fontFamily : Style.font.family
              font.pixelSize: Style.font.iconLarge
            }

            MouseArea {
              id: volumeHit
              anchors.fill: parent
              enabled: root.connected
              hoverEnabled: true
              cursorShape: Qt.PointingHandCursor
              onClicked: if (root.player) root.player.toggleMute()
            }
          }

          PanelSlider {
            id: volumeSlider
            bar: root.bar
            anchors.left: volumeButton.right
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            anchors.leftMargin: Style.space(2)
            minimum: 0
            maximum: 1
            step: 0.05
            value: root.volume
            enabled: root.connected
            opacity: root.volume <= 0.001 ? 0.45 : 1
            trackColor: Qt.rgba(root.foreground.r, root.foreground.g, root.foreground.b, 0.18)
            fillColor: Commons.Color.accent
            knobColor: Commons.Color.accent
            onMoved: function(v) { if (root.player) root.player.setVolume(v) }
            onRightClicked: if (root.player) root.player.toggleMute()
          }
        }

        Text {
          anchors.horizontalCenter: parent.horizontalCenter
          color: Ink.meta(root.foreground, Commons.Color.background)
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

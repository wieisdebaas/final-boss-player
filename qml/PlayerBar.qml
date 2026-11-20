pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons
import qs.Ui

// Bottom transport bar. Pure view over Service.playerState.
Rectangle {
  id: root

  property var service: null

  property string artPath: ""
  property int artAlbumId: -1

  readonly property var snapshot: service ? service.playerState : null
  readonly property real positionMs: snapshot ? Number(snapshot.position_ms) || 0 : 0
  readonly property real durationMs: snapshot && snapshot.duration_ms ? Number(snapshot.duration_ms) : 0
  readonly property real progress: durationMs > 0
    ? Math.max(0, Math.min(1, positionMs / durationMs)) : 0
  readonly property real volume: snapshot ? Math.max(0, Math.min(1, Number(snapshot.volume))) : 0
  readonly property var trackInfo: service ? service.track : null
  readonly property int albumId: trackInfo && trackInfo.album_id !== undefined && trackInfo.album_id !== null
    ? Number(trackInfo.album_id) : -1
  readonly property bool trackHasArt: !!trackInfo && trackInfo.has_art === true && albumId >= 0

  // Keep the footer height fixed; art fits inside this.
  implicitHeight: 72
  color: Qt.darker(Color.background, 1.15)

  function formatTime(ms) {
    var total = Math.max(0, Math.floor(ms / 1000))
    var minutes = Math.floor(total / 60)
    var seconds = total % 60
    return minutes + ":" + (seconds < 10 ? "0" : "") + seconds
  }

  function volumeGlyph() {
    if (root.volume <= 0.001) return "\u{1F507}"
    if (root.volume < 0.34) return "\u{1F508}"
    if (root.volume < 0.67) return "\u{1F509}"
    return "\u{1F50A}"
  }

  function refreshArt() {
    if (!root.service || !root.service.connected || !root.trackHasArt) {
      artPath = ""
      artAlbumId = -1
      return
    }
    if (albumId === artAlbumId) return
    var requested = albumId
    artAlbumId = requested
    artPath = ""
    root.service.request("get_art", { album_id: requested }, function(ok, result) {
      if (requested !== root.artAlbumId) return
      root.artPath = ok && result && result.path ? String(result.path) : ""
    })
  }

  onAlbumIdChanged: refreshArt()
  onTrackHasArtChanged: refreshArt()
  Connections {
    target: root.service
    function onConnectedChanged() {
      if (!root.service || !root.service.connected) {
        root.artPath = ""
        root.artAlbumId = -1
      } else {
        root.refreshArt()
      }
    }
  }
  Component.onCompleted: refreshArt()

  Rectangle {
    anchors.top: parent.top
    anchors.left: parent.left
    anchors.right: parent.right
    height: 1
    color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.12)
  }

  Row {
    id: nowPlaying
    anchors.left: parent.left
    anchors.leftMargin: 12
    anchors.verticalCenter: parent.verticalCenter
    spacing: 10
    width: Math.min(280, parent.width * 0.32)

    Rectangle {
      width: 48
      height: 48
      radius: 4
      anchors.verticalCenter: parent.verticalCenter
      color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.08)
      clip: true

      SizedImage {
        anchors.fill: parent
        crop: true
        url: root.artPath !== "" ? "file://" + root.artPath : ""
        visible: url != ""
      }

      Text {
        anchors.centerIn: parent
        visible: root.artPath === ""
        color: Color.muted
        font.pixelSize: 18
        text: "\u266A"
      }
    }

    Column {
      anchors.verticalCenter: parent.verticalCenter
      width: parent.width - 58
      spacing: 2
      Text {
        width: parent.width
        elide: Text.ElideRight
        color: Color.foreground
        font.pixelSize: 14
        font.bold: true
        text: root.service && root.service.hasMedia ? root.service.title : "Nothing playing"
      }
      Row {
        id: footerByline
        width: parent.width
        spacing: 0
        visible: root.service && root.service.hasMedia

        readonly property string artistName: root.trackInfo && root.trackInfo.artist
          ? String(root.trackInfo.artist) : ""
        readonly property string albumName: root.trackInfo && root.trackInfo.album
          ? String(root.trackInfo.album) : ""
        readonly property bool hasArtist: !!(root.trackInfo && root.trackInfo.artist_id !== undefined
          && root.trackInfo.artist_id !== null && artistName !== "")
        readonly property bool hasAlbum: !!(root.trackInfo && root.trackInfo.album_id !== undefined
          && root.trackInfo.album_id !== null && albumName !== "")

        Text {
          id: footerArtist
          elide: Text.ElideRight
          width: Math.min(implicitWidth, footerByline.width * 0.5)
          color: footerArtistArea.containsMouse ? Color.accent : Color.muted
          font.pixelSize: 12
          font.underline: footerArtistArea.containsMouse
          text: footerByline.artistName
          visible: text !== ""
          MouseArea {
            id: footerArtistArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: footerByline.hasArtist ? Qt.PointingHandCursor : Qt.ArrowCursor
            enabled: footerByline.hasArtist
            onClicked: if (root.service) root.service.openArtistFromTrack(root.trackInfo)
          }
        }
        Text {
          id: footerSep
          color: Color.muted
          font.pixelSize: 12
          text: " \u00B7 "
          visible: footerArtist.visible && footerAlbum.visible
        }
        Text {
          id: footerAlbum
          elide: Text.ElideRight
          // Text-sized hit box only — do not stretch across leftover row width.
          width: Math.min(
            implicitWidth,
            Math.max(0, footerByline.width - footerArtist.width
              - (footerSep.visible ? footerSep.width : 0)))
          color: footerAlbumArea.containsMouse ? Color.accent : Color.muted
          font.pixelSize: 12
          font.underline: footerAlbumArea.containsMouse
          text: footerByline.albumName
          visible: text !== ""
          MouseArea {
            id: footerAlbumArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: footerByline.hasAlbum ? Qt.PointingHandCursor : Qt.ArrowCursor
            enabled: footerByline.hasAlbum
            onClicked: if (root.service) root.service.openAlbumFromTrack(root.trackInfo)
          }
        }
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
          { glyph: "\u23EE", action: "previous", accent: false },
          { glyph: root.service && root.service.playing ? "\u23F8" : "\u25B6", action: "toggle", accent: false },
          { glyph: "\u23ED", action: "next", accent: false },
          { glyph: "\u21C4", action: "toggleShuffle", accent: true }
        ]
        delegate: Text {
          id: transport
          required property var modelData
          color: modelData.accent && root.service && root.service.shuffle
            ? Color.accent
            : Color.foreground
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
      font.pixelSize: 16
      opacity: root.service && root.service.connected ? 1 : 0.4
      text: root.volumeGlyph()
      MouseArea {
        anchors.fill: parent
        anchors.margins: -4
        cursorShape: Qt.PointingHandCursor
        onClicked: if (root.service) root.service.toggleMute()
      }
    }

    Rectangle {
      id: volumeTrack
      width: 100
      height: 4
      radius: 2
      anchors.verticalCenter: parent.verticalCenter
      color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.18)
      Rectangle {
        width: parent.width * root.volume
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

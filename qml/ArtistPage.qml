pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import qs.Commons

// Artists list (query kind "artists"); selecting one shows that artist's
// tracks (kind "artist", q = artist id).
Item {
  id: root

  property var service: null

  property var artists: []
  property var tracks: []
  property bool loading: false
  property string errorText: ""
  property int loadSerial: 0
  readonly property var selected: service ? service.selectedArtist : null
  readonly property int revision: service ? service.libraryRevision : 0
  readonly property bool connected: service ? service.connected : false
  readonly property var keyboardLists: selected ? [trackList] : [artistList]

  property string artPath: ""
  property int artArtistId: -1

  function refreshArt() {
    if (!selected || !service || !service.connected || selected.has_art === false) {
      artPath = ""
      artArtistId = -1
      return
    }
    var artistId = Number(selected.id)
    if (!(artistId >= 0)) return
    if (artistId === artArtistId) return
    artArtistId = artistId
    artPath = ""
    service.request("get_artist_art", { artist_id: artistId }, function(ok, result) {
      if (!root.selected || Number(root.selected.id) !== artistId) return
      root.artPath = ok && result && result.path ? String(result.path) : ""
    })
  }

  function refresh() {
    root.loadSerial += 1
    var serial = root.loadSerial
    if (!service || !service.connected) {
      artists = []
      tracks = []
      errorText = "finalbossd is not running"
      return
    }
    loading = true
    var showingArtist = !!selected
    var kind = showingArtist ? "artist" : "artists"
    var q = showingArtist ? String(selected.id) : ""
    service.queryPages(kind, q, function(ok, result, error) {
      if (serial !== root.loadSerial) return
      root.loading = false
      root.errorText = ok ? "" : error
      if (!ok) return
      if (showingArtist) root.tracks = result
      else root.artists = result
    })
  }

  onSelectedChanged: {
    tracks = []
    refreshArt()
    refresh()
  }
  onRevisionChanged: refresh()
  onConnectedChanged: {
    refreshArt()
    refresh()
  }
  Component.onCompleted: {
    refreshArt()
    refresh()
  }

  ColumnLayout {
    anchors.fill: parent
    anchors.margins: 20
    spacing: 10

    RowLayout {
      Layout.fillWidth: true
      visible: root.selected === null
      Text {
        Layout.fillWidth: true
        elide: Text.ElideRight
        color: Color.foreground
        font.pixelSize: 20
        font.bold: true
        text: "Artists"
      }
      Text {
        color: Color.muted
        font.pixelSize: 12
        text: root.loading ? "Loading\u2026" : root.artists.length + " artists"
      }

      ViewToggle {
        Layout.alignment: Qt.AlignVCenter
        service: root.service
      }
    }

    RowLayout {
      Layout.fillWidth: true
      visible: root.selected !== null
      spacing: 16

      Rectangle {
        Layout.preferredWidth: 112
        Layout.preferredHeight: 112
        Layout.alignment: Qt.AlignVCenter
        radius: 6
        clip: true
        color: Qt.rgba(Color.accent.r, Color.accent.g, Color.accent.b, 0.18)

        Image {
          anchors.fill: parent
          visible: root.artPath !== ""
          source: root.artPath !== "" ? "file://" + root.artPath : ""
          fillMode: Image.PreserveAspectCrop
          sourceSize.width: 224
          sourceSize.height: 224
          asynchronous: true
          cache: true
        }

        Text {
          anchors.centerIn: parent
          visible: root.artPath === ""
          color: Color.accent
          font.pixelSize: 42
          font.bold: true
          text: root.selected ? String(root.selected.name || "?").charAt(0).toUpperCase() : ""
        }
      }

      ColumnLayout {
        Layout.fillWidth: true
        Layout.alignment: Qt.AlignVCenter
        spacing: 4

        Text {
          color: backArea.containsMouse ? Color.accent : Color.muted
          font.pixelSize: 14
          text: "\u2190 Artists"
          MouseArea {
            id: backArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: if (root.service) root.service.navigate("artists")
          }
        }
        Text {
          Layout.fillWidth: true
          elide: Text.ElideRight
          color: Color.foreground
          font.pixelSize: 20
          font.bold: true
          text: root.selected ? String(root.selected.name || "Unknown artist") : ""
        }
        Text {
          Layout.fillWidth: true
          color: Color.muted
          font.pixelSize: 12
          text: root.loading ? "Loading\u2026" : root.tracks.length + " tracks"
        }
      }

      PillButton {
        Layout.alignment: Qt.AlignVCenter
        label: "\u25B6  Play artist"
        enabled: !!(root.service && root.selected)
        onClicked: if (root.service && root.selected)
          root.service.playArtist(Number(root.selected.id), null)
      }

      ViewToggle {
        Layout.alignment: Qt.AlignVCenter
        service: root.service
      }
    }

    Text {
      visible: root.errorText !== ""
      color: Color.urgent
      font.pixelSize: 12
      text: root.errorText
    }

    EntityList {
      id: artistList
      Layout.fillWidth: true
      Layout.fillHeight: true
      visible: root.selected === null
      items: root.artists
      service: root.service
      covers: true
      coverKind: "artist"
      playable: true
      emptyText: root.loading ? "" : "No artists yet. Add a music folder in Settings."
      secondaryOf: function(item) {
        return item.album_count + " albums \u00B7 " + item.track_count + " tracks"
      }
      onActivated: function(item) { if (root.service) root.service.openArtist(item) }
      onPlayRequested: function(item) {
        if (root.service) root.service.playArtist(Number(item.id), null)
      }
    }

    TrackList {
      id: trackList
      Layout.fillWidth: true
      Layout.fillHeight: true
      visible: root.selected !== null
      service: root.service
      tracks: root.tracks
      queueScope: "artist"
      revealPage: "artists"
      showNumbers: false
      emptyText: root.loading ? "" : "No tracks"
    }
  }
}

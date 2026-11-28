pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import qs.Commons
import "ink.js" as Ink

// Genres list (query kind "genres"); selecting one shows that genre's tracks
// (kind "genre", q = genre name). List and tiles follow the shared view toggle.
Item {
  id: root

  property var service: null

  property var genres: []
  property var tracks: []
  property bool loading: false
  property string errorText: ""
  property int loadSerial: 0
  readonly property var selected: service ? service.selectedGenre : null
  readonly property int revision: service ? service.libraryRevision : 0
  readonly property bool connected: service ? service.connected : false
  readonly property var keyboardLists: selected ? [trackList] : [genreList]

  property string artPath: ""
  property string artGenre: ""

  function refreshArt() {
    if (!selected || !service || !service.connected || selected.has_art === false) {
      artPath = ""
      artGenre = ""
      return
    }
    var name = String(selected.name || "")
    if (name === "") return
    if (name === artGenre) return
    artGenre = name
    artPath = ""
    service.request("get_genre_art", { name: name }, function(ok, result) {
      if (!root.selected || String(root.selected.name || "") !== name) return
      root.artPath = ok && result && result.path ? String(result.path) : ""
    })
  }

  function refresh() {
    root.loadSerial += 1
    var serial = root.loadSerial
    if (!service || !service.connected) {
      genres = []
      tracks = []
      errorText = "finalbossd is not running"
      return
    }
    loading = true
    var showingGenre = !!selected
    var kind = showingGenre ? "genre" : "genres"
    var q = showingGenre ? String(selected.name || "") : ""
    service.queryPages(kind, q, function(ok, result, error) {
      if (serial !== root.loadSerial) return
      root.loading = false
      root.errorText = ok ? "" : error
      if (!ok) return
      if (showingGenre) root.tracks = result
      else root.genres = result
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
        color: Ink.title(Color.foreground, Color.background)
        font.pixelSize: 20
        font.bold: true
        text: "Genres"
      }
      Text {
        color: Ink.meta(Color.foreground, Color.background)
        font.pixelSize: 12
        text: root.loading ? "Loading\u2026" : root.genres.length + " genres"
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
          color: backArea.containsMouse ? Color.accent : Ink.meta(Color.foreground, Color.background)
          font.pixelSize: 14
          text: "\u2190 Genres"
          MouseArea {
            id: backArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: if (root.service) root.service.navigate("genres")
          }
        }
        Text {
          Layout.fillWidth: true
          elide: Text.ElideRight
          color: Ink.title(Color.foreground, Color.background)
          font.pixelSize: 20
          font.bold: true
          text: root.selected ? String(root.selected.name || "Unknown genre") : ""
        }
        Text {
          Layout.fillWidth: true
          color: Ink.meta(Color.foreground, Color.background)
          font.pixelSize: 12
          text: root.loading ? "Loading\u2026" : root.tracks.length + " tracks"
        }
      }

      PillButton {
        Layout.alignment: Qt.AlignVCenter
        label: "\u25B6  Play genre"
        enabled: !!(root.service && root.selected)
        onClicked: if (root.service && root.selected)
          root.service.playGenre(String(root.selected.name || ""), null)
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
      id: genreList
      Layout.fillWidth: true
      Layout.fillHeight: true
      visible: root.selected === null
      items: root.genres
      service: root.service
      covers: true
      coverKind: "genre"
      playable: true
      emptyText: root.loading ? "" : "No genres yet. Tags are read from your files, or filled in from Settings."
      secondaryOf: function(item) {
        return item.album_count + " albums \u00B7 " + item.track_count + " tracks"
      }
      onActivated: function(item) { if (root.service) root.service.openGenre(item) }
      onPlayRequested: function(item) {
        if (root.service) root.service.playGenre(String(item.name || ""), null)
      }
    }

    TrackList {
      id: trackList
      Layout.fillWidth: true
      Layout.fillHeight: true
      visible: root.selected !== null
      service: root.service
      tracks: root.tracks
      queueScope: "genre"
      revealPage: "genres"
      showNumbers: false
      emptyText: root.loading ? "" : "No tracks"
    }
  }
}

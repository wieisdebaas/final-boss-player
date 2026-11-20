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
  readonly property var selected: service ? service.selectedArtist : null
  readonly property int revision: service ? service.libraryRevision : 0
  readonly property bool connected: service ? service.connected : false

  function refresh() {
    if (!service || !service.connected) {
      artists = []
      tracks = []
      errorText = "endbossd is not running"
      return
    }
    loading = true
    if (selected) {
      service.query("artist", String(selected.id), 500, 0, function(ok, result, error) {
        root.loading = false
        root.errorText = ok ? "" : error
        if (ok) root.tracks = Array.isArray(result) ? result : []
      })
    } else {
      service.query("artists", "", 500, 0, function(ok, result, error) {
        root.loading = false
        root.errorText = ok ? "" : error
        if (ok) root.artists = Array.isArray(result) ? result : []
      })
    }
  }

  onSelectedChanged: {
    tracks = []
    refresh()
  }
  onRevisionChanged: refresh()
  onConnectedChanged: refresh()
  Component.onCompleted: refresh()

  ColumnLayout {
    anchors.fill: parent
    anchors.margins: 20
    spacing: 10

    RowLayout {
      Layout.fillWidth: true
      spacing: 12

      Text {
        visible: root.selected !== null
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
        text: root.selected ? String(root.selected.name || "Unknown artist") : "Artists"
      }
      Text {
        color: Color.muted
        font.pixelSize: 12
        text: root.loading ? "Loading\u2026"
          : (root.selected ? root.tracks.length + " tracks" : root.artists.length + " artists")
      }
    }

    Text {
      visible: root.errorText !== ""
      color: Color.urgent
      font.pixelSize: 12
      text: root.errorText
    }

    EntityList {
      Layout.fillWidth: true
      Layout.fillHeight: true
      visible: root.selected === null
      items: root.artists
      emptyText: root.loading ? "" : "No artists yet. Add a music folder in Settings."
      secondaryOf: function(item) {
        return item.album_count + " albums \u00B7 " + item.track_count + " tracks"
      }
      onActivated: function(item) { if (root.service) root.service.openArtist(item) }
    }

    TrackList {
      Layout.fillWidth: true
      Layout.fillHeight: true
      visible: root.selected !== null
      service: root.service
      tracks: root.tracks
      showNumbers: false
      emptyText: root.loading ? "" : "No tracks"
    }
  }
}

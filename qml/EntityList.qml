pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import qs.Commons

// Generic album/artist rows. `items` entries are daemon objects; `secondaryOf`
// builds the subtitle. Emits activated(item) on click and playRequested(item)
// when the play glyph is clicked (only shown when `playable`).
ListView {
  id: root

  property var items: []
  property var service: null
  // Album rows only. Artists have no picture in the library.
  property bool covers: false
  property bool playable: false
  property string emptyText: "Nothing here"
  property var secondaryOf: function(item) { return "" }

  signal activated(var item)
  signal playRequested(var item)

  clip: true
  model: items
  boundsBehavior: Flickable.StopAtBounds
  ScrollBar.vertical: ScrollBar {
    policy: ScrollBar.AsNeeded
    width: 10
    contentItem: Rectangle {
      implicitWidth: 6
      radius: 3
      color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b,
                     parent.hovered || parent.pressed ? 0.45 : 0.28)
    }
  }

  delegate: Rectangle {
    id: row
    required property var modelData

    width: ListView.view.width
    height: 52
    radius: 6
    color: rowArea.containsMouse
      ? Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.08)
      : "transparent"

    property string artPath: ""
    property int artAlbumId: -1

    function refreshArt() {
      if (!root.covers || !root.service || !root.service.connected || row.modelData.has_art !== true) {
        artPath = ""
        artAlbumId = -1
        return
      }
      var albumId = Number(row.modelData.id)
      if (!(albumId >= 0)) return
      if (albumId === artAlbumId) return
      artAlbumId = albumId
      artPath = ""
      root.service.request("get_art", { album_id: albumId }, function(ok, result) {
        if (albumId !== row.artAlbumId) return
        row.artPath = ok && result && result.path ? String(result.path) : ""
      })
    }

    onModelDataChanged: refreshArt()
    Component.onCompleted: refreshArt()

    Rectangle {
      id: cover
      anchors.left: parent.left
      anchors.leftMargin: 10
      anchors.verticalCenter: parent.verticalCenter
      width: 36
      height: 36
      radius: 4
      clip: true
      color: Qt.rgba(Color.accent.r, Color.accent.g, Color.accent.b, 0.18)

      Image {
        anchors.fill: parent
        visible: row.artPath !== ""
        source: row.artPath !== "" ? "file://" + row.artPath : ""
        fillMode: Image.PreserveAspectCrop
        sourceSize.width: 72
        sourceSize.height: 72
        asynchronous: true
        cache: true
      }

      Text {
        anchors.centerIn: parent
        visible: row.artPath === ""
        color: Color.accent
        font.pixelSize: 15
        font.bold: true
        text: String(row.modelData.name || "?").charAt(0).toUpperCase()
      }
    }

    Column {
      anchors.left: cover.right
      anchors.leftMargin: 12
      anchors.right: playGlyph.left
      anchors.rightMargin: 8
      anchors.verticalCenter: parent.verticalCenter
      spacing: 1
      Text {
        width: parent.width
        elide: Text.ElideRight
        color: Color.foreground
        font.pixelSize: 13
        text: String(row.modelData.name || "Unknown")
      }
      Text {
        width: parent.width
        elide: Text.ElideRight
        color: Color.muted
        font.pixelSize: 11
        text: root.secondaryOf(row.modelData)
      }
    }

    MouseArea {
      id: rowArea
      anchors.fill: parent
      hoverEnabled: true
      cursorShape: Qt.PointingHandCursor
      onClicked: root.activated(row.modelData)
    }

    IconButton {
      id: playGlyph
      anchors.right: parent.right
      anchors.rightMargin: 10
      anchors.verticalCenter: parent.verticalCenter
      visible: root.playable
      glyph: "\u25B6"
      glyphOffset: 1
      onClicked: root.playRequested(row.modelData)
    }
  }

  Text {
    anchors.centerIn: parent
    visible: root.count === 0
    color: Color.muted
    font.pixelSize: 13
    text: root.emptyText
  }
}

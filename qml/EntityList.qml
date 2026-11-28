pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import qs.Commons
import "listkeys.js" as ListKeys
import "ink.js" as Ink

// Album and artist rows, or a cover grid when the service is in tiles mode.
// `secondaryOf` builds the subtitle. Emits activated(item) on click and
// playRequested(item) when the play glyph is clicked (only when `playable`).
Item {
  id: root

  property var items: []
  property var service: null
  // Album rows use get_art. Artist rows use get_artist_art. Genre rows use get_genre_art.
  property bool covers: false
  property string coverKind: "album"
  property bool playable: false
  property string emptyText: "Nothing here"
  property var secondaryOf: function(item) { return "" }

  readonly property bool tiled: !!(service && service.tiles)
  readonly property int columnCount: Math.max(2, Math.floor(Math.max(width, 1) / 156))
  readonly property int cellWidth: Math.max(1, Math.floor(width / columnCount))
  readonly property int cellHeight: cellWidth + 46

  signal activated(var item)
  signal playRequested(var item)

  property int keyboardIndex: -1
  property bool keyboardFocused: false
  readonly property int keyboardCount: Array.isArray(items) ? items.length : 0
  readonly property int keyboardStride: tiled ? cellHeight : 52
  readonly property color titleColor: Ink.title(Color.foreground, Color.background)
  readonly property color metaColor: Ink.meta(Color.foreground, Color.background)
  readonly property color quietColor: Ink.quiet(Color.foreground, Color.background)

  function publishKeyboard() {
    if (keyboardIndex >= keyboardCount)
      keyboardIndex = keyboardCount > 0 ? keyboardCount - 1 : -1
    if (service) service.noteKeyboard()
  }

  function revealKeyboard() {
    if (keyboardIndex < 0) return
    var view = tiled ? grid : rows
    view.positionViewAtIndex(keyboardIndex, tiled ? GridView.Contain : ListView.Contain)
  }

  function moveKeyboard(dx, dy) {
    var result = ListKeys.move(keyboardIndex, keyboardCount,
      ListKeys.columnsOf(tiled, columnCount), dx, dy)
    if (result.edge === "") {
      keyboardIndex = result.index
      revealKeyboard()
    }
    return result.edge
  }

  function pageKeyboard(direction) {
    var rowsHigh = Math.max(1, Math.floor(height / Math.max(1, keyboardStride)))
    keyboardIndex = ListKeys.page(keyboardIndex, keyboardCount,
      ListKeys.columnsOf(tiled, columnCount), rowsHigh, direction)
    revealKeyboard()
  }

  function jumpKeyboardStart() {
    keyboardIndex = keyboardCount > 0 ? 0 : -1
    revealKeyboard()
  }

  function jumpKeyboardEnd() {
    keyboardIndex = keyboardCount > 0 ? keyboardCount - 1 : -1
    revealKeyboard()
  }

  function activateKeyboard() {
    if (keyboardIndex < 0 || keyboardIndex >= keyboardCount) return
    activated(items[keyboardIndex])
  }

  onKeyboardCountChanged: publishKeyboard()
  onVisibleChanged: publishKeyboard()
  onTiledChanged: {
    revealKeyboard()
    publishKeyboard()
  }

  function entityKey(item) {
    if (coverKind === "genre") return "genre:" + String(item && item.name || "")
    return coverKind + ":" + String(item && item.id)
  }

  function artCommand(item) {
    if (coverKind === "artist") return ["get_artist_art", { artist_id: Number(item.id) }]
    if (coverKind === "genre") return ["get_genre_art", { name: String(item.name || "") }]
    return ["get_art", { album_id: Number(item.id) }]
  }

  ListView {
    id: rows
    anchors.fill: parent
    visible: !root.tiled
    clip: true
    model: root.tiled ? [] : root.items
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
    delegate: rowDelegate
  }

  GridView {
    id: grid
    anchors.fill: parent
    visible: root.tiled
    clip: true
    model: root.tiled ? root.items : []
    cellWidth: root.cellWidth
    cellHeight: root.cellHeight
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
    delegate: tileDelegate
  }

  Component {
    id: rowDelegate
    Rectangle {
      id: row
      required property var modelData
      required property int index
      readonly property bool keyboardHere: root.keyboardFocused && index === root.keyboardIndex

      width: ListView.view.width
      height: 52
      radius: 6
      border.width: row.keyboardHere ? 1 : 0
      border.color: Color.accent
      color: row.keyboardHere
        ? Qt.rgba(Color.accent.r, Color.accent.g, Color.accent.b, 0.2)
        : (rowArea.containsMouse
          ? Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.08)
          : "transparent")

      property string artPath: ""
      property string artKey: ""

      function refreshArt() {
        if (!root.covers || !root.service || !root.service.connected || row.modelData.has_art !== true) {
          artPath = ""
          artKey = ""
          return
        }
        var key = root.entityKey(row.modelData)
        if (key === artKey) return
        artKey = key
        artPath = ""
        var request = root.artCommand(row.modelData)
        root.service.request(request[0], request[1], function(ok, result) {
          if (key !== row.artKey) return
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
          color: root.titleColor
          font.pixelSize: 13
          text: String(row.modelData.name || "Unknown")
        }
        Text {
          width: parent.width
          elide: Text.ElideRight
          color: root.metaColor
          font.pixelSize: 12
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
  }

  Component {
    id: tileDelegate
    Item {
      id: tile
      required property var modelData
      required property int index
      readonly property bool keyboardHere: root.keyboardFocused && index === root.keyboardIndex

      width: GridView.view.cellWidth
      height: GridView.view.cellHeight

      property string artPath: ""
      property string artKey: ""

      function refreshArt() {
        if (!root.covers || !root.service || !root.service.connected || tile.modelData.has_art !== true) {
          artPath = ""
          artKey = ""
          return
        }
        var key = root.entityKey(tile.modelData)
        if (key === artKey) return
        artKey = key
        artPath = ""
        var request = root.artCommand(tile.modelData)
        root.service.request(request[0], request[1], function(ok, result) {
          if (key !== tile.artKey) return
          tile.artPath = ok && result && result.path ? String(result.path) : ""
        })
      }

      onModelDataChanged: refreshArt()
      Component.onCompleted: refreshArt()

      Rectangle {
        id: card
        anchors.fill: parent
        anchors.margins: 6
        radius: 8
        color: tile.keyboardHere
          ? Qt.rgba(Color.accent.r, Color.accent.g, Color.accent.b, 0.2)
          : (tileArea.containsMouse
            ? Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.08)
            : "transparent")
        border.width: tile.keyboardHere ? 1 : 0
        border.color: Color.accent

        Rectangle {
          id: cover
          anchors.top: parent.top
          anchors.left: parent.left
          anchors.right: parent.right
          anchors.margins: 6
          height: width
          radius: 6
          clip: true
          color: Qt.rgba(Color.accent.r, Color.accent.g, Color.accent.b, 0.18)

          Image {
            anchors.fill: parent
            visible: tile.artPath !== ""
            source: tile.artPath !== "" ? "file://" + tile.artPath : ""
            fillMode: Image.PreserveAspectCrop
            sourceSize.width: 280
            sourceSize.height: 280
            asynchronous: true
            cache: true
          }

          Text {
            anchors.centerIn: parent
            visible: tile.artPath === ""
            color: Color.accent
            font.pixelSize: 28
            font.bold: true
            text: String(tile.modelData.name || "?").charAt(0).toUpperCase()
          }
        }

        Column {
          anchors.top: cover.bottom
          anchors.topMargin: 6
          anchors.left: parent.left
          anchors.right: parent.right
          anchors.leftMargin: 8
          anchors.rightMargin: 8
          spacing: 1
          Text {
            width: parent.width
            elide: Text.ElideRight
            color: root.titleColor
            font.pixelSize: 13
            text: String(tile.modelData.name || "Unknown")
          }
          Text {
            width: parent.width
            elide: Text.ElideRight
            color: root.metaColor
            font.pixelSize: 12
            text: root.secondaryOf(tile.modelData)
          }
        }

        MouseArea {
          id: tileArea
          anchors.fill: parent
          hoverEnabled: true
          cursorShape: Qt.PointingHandCursor
          onClicked: root.activated(tile.modelData)
        }

        IconButton {
          anchors.right: cover.right
          anchors.bottom: cover.bottom
          anchors.margins: 6
          visible: root.playable
          diameter: 28
          glyph: "\u25B6"
          glyphOffset: 1
          onClicked: root.playRequested(tile.modelData)
        }
      }
    }
  }

  Text {
    anchors.centerIn: parent
    visible: !root.items || root.items.length === 0
    color: root.quietColor
    font.pixelSize: 13
    text: root.emptyText
  }
}

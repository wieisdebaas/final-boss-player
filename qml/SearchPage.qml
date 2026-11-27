pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import qs.Commons

// Debounced track search (query kind "search"). Ctrl+F focuses the field.
Item {
  id: root

  property var service: null

  property var results: []
  property bool loading: false
  property string errorText: ""
  property int requestSerial: 0
  readonly property string queryText: service ? service.searchQuery : ""
  readonly property int focusRequest: service ? service.searchFocusRequest : 0
  readonly property bool connected: service ? service.connected : false
  readonly property var keyboardLists: [resultList]

  function editingInput() { return input }

  function runSearch() {
    var text = queryText.trim()
    requestSerial++
    var serial = requestSerial
    if (text === "") {
      results = []
      loading = false
      errorText = ""
      return
    }
    if (!service || !service.connected) {
      results = []
      errorText = "finalbossd is not running"
      return
    }
    loading = true
    service.query("search", text, 100, 0, function(ok, result, error) {
      if (serial !== root.requestSerial) return  // stale response
      root.loading = false
      if (!ok) {
        root.errorText = error
        return
      }
      root.errorText = ""
      root.results = Array.isArray(result) ? result : []
    })
  }

  onQueryTextChanged: debounce.restart()
  onConnectedChanged: runSearch()
  onFocusRequestChanged: Qt.callLater(function() {
    input.forceActiveFocus()
    input.selectAll()
  })
  onVisibleChanged: if (visible) Qt.callLater(function() { input.forceActiveFocus() })
  Component.onCompleted: runSearch()

  Timer {
    id: debounce
    interval: 200
    onTriggered: root.runSearch()
  }

  ColumnLayout {
    anchors.fill: parent
    anchors.margins: 20
    spacing: 10

    RowLayout {
      Layout.fillWidth: true
      Text {
        Layout.fillWidth: true
        elide: Text.ElideRight
        color: Color.foreground
        font.pixelSize: 20
        font.bold: true
        text: "Search"
      }
      ViewToggle {
        Layout.alignment: Qt.AlignVCenter
        service: root.service
      }
    }

    Rectangle {
      Layout.fillWidth: true
      implicitHeight: 36
      radius: 6
      color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.08)
      border.width: input.activeFocus ? 1 : 0
      border.color: Color.accent

      TextInput {
        id: input
        anchors.fill: parent
        anchors.leftMargin: 12
        anchors.rightMargin: 12
        verticalAlignment: TextInput.AlignVCenter
        color: Color.foreground
        selectionColor: Color.accent
        selectedTextColor: Color.background
        font.pixelSize: 13
        clip: true
        text: root.queryText
        onTextChanged: if (root.service && root.service.searchQuery !== text) root.service.searchQuery = text
        onActiveFocusChanged: if (root.service) root.service.setTextEditing(activeFocus)

        Text {
          anchors.verticalCenter: parent.verticalCenter
          visible: input.text === ""
          color: Color.muted
          font.pixelSize: 13
          text: "Tracks, artists, albums \u2014 separate with commas"
        }
      }
    }

    Text {
      color: root.errorText !== "" ? Color.urgent : Color.muted
      font.pixelSize: 12
      visible: root.errorText !== "" || root.loading || root.queryText.trim() !== ""
      text: root.errorText !== "" ? root.errorText
        : (root.loading ? "Searching\u2026" : root.results.length + " results")
    }

    TrackList {
      id: resultList
      Layout.fillWidth: true
      Layout.fillHeight: true
      service: root.service
      tracks: root.results
      showNumbers: false
      emptyText: root.queryText.trim() === "" ? "Type to search your library" : "No matches"
    }
  }
}

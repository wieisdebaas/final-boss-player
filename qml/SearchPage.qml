pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
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
  // Set when Enter should move to the first result once this query is showing.
  property bool jumpWhenReady: false
  property string committedQuery: ""
  property string shownQuery: ""
  property string matchField: "all"
  property string genreFilter: ""
  readonly property string queryText: service ? service.searchQuery : ""
  readonly property string searchKey: queryText.trim() + "\n" + matchField + "\n" + genreFilter
  readonly property int focusRequest: service ? service.searchFocusRequest : 0
  readonly property bool connected: service ? service.connected : false
  readonly property var keyboardLists: [resultList]

  signal resultsCommitted()

  function editingInput() { return input }

  function runSearch() {
    var text = queryText.trim()
    var key = searchKey
    requestSerial++
    var serial = requestSerial
    if (text === "" && genreFilter === "") {
      results = []
      shownQuery = ""
      loading = false
      errorText = ""
      jumpWhenReady = false
      return
    }
    if (!service || !service.connected) {
      results = []
      shownQuery = ""
      errorText = "finalbossd is not running"
      jumpWhenReady = false
      return
    }
    loading = true
    service.querySearch(text, matchField, genreFilter, 200, 0, function(ok, result, error) {
      if (serial !== root.requestSerial) return  // stale response
      root.loading = false
      if (!ok) {
        root.errorText = error
        if (root.jumpWhenReady && key === root.committedQuery) root.jumpWhenReady = false
        return
      }
      root.errorText = ""
      root.results = Array.isArray(result) ? result : []
      root.shownQuery = key
      if (root.jumpWhenReady && key === root.committedQuery && key === root.searchKey) {
        root.jumpWhenReady = false
        if (root.results.length > 0) root.resultsCommitted()
      }
    })
  }

  // Enter means the query is finished. Jump when this text is already on
  // screen; otherwise search now and jump when that response arrives.
  function commitSearch() {
    debounce.stop()
    var key = searchKey
    if (queryText.trim() === "" && genreFilter === "") {
      jumpWhenReady = false
      return
    }
    if (!loading && shownQuery === key) {
      jumpWhenReady = false
      if (results.length > 0) resultsCommitted()
      return
    }
    committedQuery = key
    jumpWhenReady = true
    runSearch()
  }

  function applyFilter() {
    if (searchKey !== committedQuery) jumpWhenReady = false
    debounce.stop()
    runSearch()
  }

  onQueryTextChanged: {
    if (searchKey !== committedQuery) jumpWhenReady = false
    debounce.restart()
  }
  onMatchFieldChanged: applyFilter()
  onGenreFilterChanged: applyFilter()
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
        onAccepted: root.commitSearch()
        onActiveFocusChanged: if (root.service) root.service.setTextEditing(activeFocus)

        Text {
          anchors.verticalCenter: parent.verticalCenter
          visible: input.text === ""
          color: Color.muted
          font.pixelSize: 13
          text: "Tracks, artists, albums, genres \u2014 separate with commas"
        }
      }
    }

    RowLayout {
      Layout.fillWidth: true
      spacing: 6

      Text {
        color: Color.muted
        font.pixelSize: 12
        text: "Match"
      }

      Repeater {
        model: [
          { field: "all", label: "All" },
          { field: "title", label: "Title" },
          { field: "artist", label: "Artist" },
          { field: "album", label: "Album" },
          { field: "genre", label: "Genre" }
        ]
        delegate: Rectangle {
          id: matchChip
          required property var modelData
          readonly property bool on: root.matchField === modelData.field
          Layout.alignment: Qt.AlignVCenter
          implicitWidth: matchLabel.implicitWidth + 16
          height: 26
          radius: 13
          color: on
            ? Qt.rgba(Color.accent.r, Color.accent.g, Color.accent.b, 0.22)
            : "transparent"
          border.width: 1
          border.color: on
            ? Color.accent
            : Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.28)
          Text {
            id: matchLabel
            anchors.centerIn: parent
            text: matchChip.modelData.label
            color: matchChip.on ? Color.accent : Color.foreground
            font.pixelSize: 12
          }
          MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: root.matchField = matchChip.modelData.field
          }
        }
      }

      Item { Layout.fillWidth: true }

      Text {
        color: Color.muted
        font.pixelSize: 12
        text: "Genre"
      }

      ComboBox {
        id: genreBox
        Layout.preferredWidth: 148
        Layout.preferredHeight: 28
        model: genreModel
        textRole: "name"
        valueRole: "family"
        font.pixelSize: 12
        onActivated: root.genreFilter = currentValue || ""

        contentItem: Text {
          leftPadding: 10
          rightPadding: 18
          text: genreBox.displayText
          color: Color.foreground
          font.pixelSize: 12
          verticalAlignment: Text.AlignVCenter
          elide: Text.ElideRight
        }
        indicator: Text {
          x: genreBox.width - width - 8
          anchors.verticalCenter: parent.verticalCenter
          text: "\u25BE"
          color: Color.muted
          font.pixelSize: 10
        }
        background: Rectangle {
          radius: 6
          color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.08)
          border.width: genreBox.hovered || genreBox.pressed ? 1 : 0
          border.color: Color.accent
        }
        delegate: ItemDelegate {
          required property var model
          required property int index
          width: genreBox.width
          height: 28
          highlighted: genreBox.highlightedIndex === index
          contentItem: Text {
            text: model.name
            color: Color.foreground
            font.pixelSize: 12
            verticalAlignment: Text.AlignVCenter
            leftPadding: 10
          }
          background: Rectangle {
            color: parent.highlighted
              ? Qt.rgba(Color.accent.r, Color.accent.g, Color.accent.b, 0.22)
              : "transparent"
          }
        }
        popup: Popup {
          y: genreBox.height + 4
          width: genreBox.width
          implicitHeight: Math.min(320, contentItem.implicitHeight + 8)
          padding: 4
          contentItem: ListView {
            clip: true
            implicitHeight: contentHeight
            model: genreBox.delegateModel
            currentIndex: genreBox.highlightedIndex
          }
          background: Rectangle {
            radius: 6
            color: Qt.darker(Color.background, 1.2)
            border.width: 1
            border.color: Color.accent
          }
        }
      }
    }

    ListModel {
      id: genreModel
      ListElement { family: ""; name: "Any" }
      ListElement { family: "hip hop"; name: "Hip-Hop" }
      ListElement { family: "metal"; name: "Metal" }
      ListElement { family: "rock"; name: "Rock" }
      ListElement { family: "punk"; name: "Punk" }
      ListElement { family: "pop"; name: "Pop" }
      ListElement { family: "electronic"; name: "Electronic" }
      ListElement { family: "r&b"; name: "R&B / Soul" }
      ListElement { family: "blues"; name: "Blues" }
      ListElement { family: "country"; name: "Country" }
      ListElement { family: "jazz"; name: "Jazz" }
      ListElement { family: "reggae"; name: "Reggae" }
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
      queueScope: "search"
      revealPage: "search"
      showNumbers: false
      covers: true
      emptyText: root.queryText.trim() === "" ? "Type to search your library" : "No matches"
    }
  }
}

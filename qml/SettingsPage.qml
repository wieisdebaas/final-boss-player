pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import Quickshell
import qs.Commons

// Music folders: add (starts a scan), remove, rescan.
Item {
  id: root

  property var service: null

  property string statusText: ""
  property bool statusIsError: false

  readonly property var scan: service ? service.playerState.scan : null
  readonly property string scanText: {
    if (!scan) return ""
    var done = Number(scan.scanned) || 0
    var total = Number(scan.total) || 0
    if (scan.scanning) return "Scanning\u2026 " + done + (total > 0 ? " / " + total : "")
    return "Last scan indexed " + done + " files"
  }
  readonly property string metadataText: {
    var meta = root.service && root.service.playerState ? root.service.playerState.metadata : null
    if (!meta) return ""
    var looked = Number(meta.looked) || 0
    var total = Number(meta.total) || 0
    if (meta.looking) return "Looking up metadata\u2026 " + looked + " / " + total
    if (total > 0) return "Metadata lookup finished (" + total + " albums)"
    return ""
  }

  function editingInput() { return pathInput }

  function report(ok, error, success) {
    statusIsError = !ok
    statusText = ok ? success : String(error || "Request failed")
  }

  function scrollBy(delta) {
    var maxY = Math.max(0, flick.contentHeight - flick.height)
    flick.contentY = Math.max(0, Math.min(maxY, flick.contentY + delta))
  }

  function scrollTo(top) {
    flick.contentY = top ? 0 : Math.max(0, flick.contentHeight - flick.height)
  }

  readonly property int scrollPage: Math.max(48, Math.floor(flick.height * 0.9))
  // One width for every settings action, so the pills share a right-hand column.
  // The form stays a fixed column, the same 420 the playback bar uses, plus that
  // button. It does not stretch to the page edge.
  readonly property int actionGap: 12
  readonly property int actionWidth: actionMeasure.implicitWidth + 28
  readonly property int fieldWidth: 420
  readonly property int formWidth: fieldWidth + actionGap + actionWidth

  function submitFolder() {
    var path = pathInput.text.trim()
    if (path === "") return
    // The daemon canonicalizes paths but does not expand "~".
    var home = String(Quickshell.env("HOME") || "")
    if (home !== "" && (path === "~" || path.indexOf("~/") === 0))
      path = home + path.substring(1)
    if (!service) return
    service.addFolder(path, function(ok, result, error) {
      root.report(ok, error, ok && result && result.path
        ? "Added " + result.path : "Folder added")
      if (ok) pathInput.text = ""
    })
  }

  Text {
    id: actionMeasure
    visible: false
    font.pixelSize: 13
    font.bold: true
    text: "Same volume off"
  }

  Flickable {
    id: flick
    anchors.fill: parent
    clip: true
    contentWidth: width
    contentHeight: column.y + column.implicitHeight + 20
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

    Column {
    id: column
    x: 20
    y: 20
    width: flick.width - 40
    spacing: 16

    Row {
      width: parent.width
      spacing: 12
      Text {
        width: Math.max(0, parent.width - viewToggle.width - 12)
        height: viewToggle.height
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
        color: Color.foreground
        font.pixelSize: 20
        font.bold: true
        text: "Settings"
      }
      ViewToggle {
        id: viewToggle
        service: root.service
      }
    }

    Text {
      color: Color.muted
      font.pixelSize: 12
      text: root.service && root.service.connected
        ? "Connected to finalbossd"
        : "finalbossd is not running \u2014 start it, then reconnect."
    }

    Item {
      visible: !!(root.service && !root.service.connected)
      width: Math.min(root.formWidth, parent.width)
      height: visible ? reconnectButton.height : 0
      PillButton {
        id: reconnectButton
        anchors.right: parent.right
        width: root.actionWidth
        label: "Reconnect"
        onClicked: root.service.reconnect()
      }
    }

    Text {
      color: Color.foreground
      font.pixelSize: 14
      font.bold: true
      text: "Playback"
    }

    Row {
      width: Math.min(root.formWidth, parent.width)
      spacing: root.actionGap
      Text {
        width: Math.max(0, parent.width - root.actionWidth - root.actionGap)
        anchors.verticalCenter: parent.verticalCenter
        wrapMode: Text.Wrap
        color: Color.muted
        font.pixelSize: 12
        text: "Play each song at a similar loudness. The volume slider still sets the overall level."
      }
      PillButton {
        id: sameVolume
        width: root.actionWidth
        filled: !!(root.service && root.service.normalize)
        label: root.service && root.service.normalize ? "Same volume on" : "Same volume off"
        enabled: !!(root.service && root.service.connected)
        onClicked: if (root.service) root.service.setNormalize(!root.service.normalize)
      }
    }

    Text {
      color: Color.foreground
      font.pixelSize: 14
      font.bold: true
      text: "Music folders"
    }

    Row {
      width: Math.min(root.formWidth, parent.width)
      spacing: root.actionGap
      Rectangle {
        width: Math.max(0, parent.width - root.actionWidth - root.actionGap)
        height: 34
        radius: 6
        color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.08)
        border.width: pathInput.activeFocus ? 1 : 0
        border.color: Color.accent
        TextInput {
          id: pathInput
          anchors.fill: parent
          anchors.leftMargin: 10
          anchors.rightMargin: 10
          verticalAlignment: TextInput.AlignVCenter
          color: Color.foreground
          font.pixelSize: 13
          clip: true
          selectByMouse: true
          onAccepted: root.submitFolder()
          onActiveFocusChanged: if (root.service) root.service.setTextEditing(activeFocus)
          Text {
            anchors.fill: parent
            verticalAlignment: Text.AlignVCenter
            color: Color.muted
            font.pixelSize: 13
            visible: pathInput.text === "" && !pathInput.activeFocus
            text: "/path/to/Music"
          }
        }
      }
      PillButton {
        width: root.actionWidth
        label: "Add folder"
        enabled: !!(root.service && root.service.connected)
        onClicked: root.submitFolder()
      }
    }

    Repeater {
      model: root.service ? root.service.folders : []
      delegate: Row {
        id: folderRow
        required property var modelData
        width: Math.min(root.formWidth, column.width)
        spacing: root.actionGap
        Text {
          anchors.verticalCenter: parent.verticalCenter
          width: Math.max(0, parent.width - root.actionWidth - root.actionGap)
          elide: Text.ElideMiddle
          color: Color.foreground
          font.pixelSize: 13
          text: String(folderRow.modelData)
        }
        PillButton {
          width: root.actionWidth
          filled: false
          label: "Remove"
          onClicked: root.service.removeFolder(String(folderRow.modelData),
            function(ok, result, error) {
              root.report(ok, error, "Removed folder")
            })
        }
      }
    }

    Row {
      id: metadataRow
      width: Math.min(root.formWidth, parent.width)
      spacing: 12

      Item {
        id: metadataToggle
        width: metaBox.width + metaLabel.implicitWidth + 8
        height: 34
        anchors.verticalCenter: parent.verticalCenter
        opacity: enabled ? 1 : 0.4
        property bool enabled: !!(root.service && root.service.connected)

        Rectangle {
          id: metaBox
          width: 18
          height: 18
          anchors.verticalCenter: parent.verticalCenter
          radius: 3
          border.width: 1
          border.color: Color.accent
          color: root.service && root.service.updateMetadata ? Color.accent : "transparent"
          Text {
            anchors.centerIn: parent
            visible: !!(root.service && root.service.updateMetadata)
            text: "\u2713"
            color: Color.background
            font.pixelSize: 12
          }
        }
        Text {
          id: metaLabel
          anchors.left: metaBox.right
          anchors.leftMargin: 8
          anchors.verticalCenter: parent.verticalCenter
          color: Color.foreground
          font.pixelSize: 13
          text: "Update metadata while scanning"
        }
        MouseArea {
          anchors.fill: parent
          enabled: metadataToggle.enabled
          cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
          onClicked: if (root.service)
            root.service.setUpdateMetadata(!root.service.updateMetadata)
        }
      }

      Text {
        width: Math.max(0, metadataRow.width - metadataToggle.width - metadataRow.spacing)
        anchors.verticalCenter: parent.verticalCenter
        wrapMode: Text.Wrap
        color: Color.muted
        font.pixelSize: 12
        text: "Fill in missing genre and album art from MusicBrainz. Your files are not modified."
      }
    }

    Row {
      id: rescanRow
      width: Math.min(root.formWidth, parent.width)
      spacing: root.actionGap
      Text {
        width: Math.max(0, rescanRow.width - root.actionWidth - root.actionGap)
        anchors.verticalCenter: parent.verticalCenter
        wrapMode: Text.Wrap
        color: Color.muted
        font.pixelSize: 12
        text: root.metadataText !== ""
          ? root.scanText + "  \u00B7  " + root.metadataText
          : root.scanText
      }
      PillButton {
        width: root.actionWidth
        filled: false
        label: "Rescan"
        enabled: !!(root.service && root.service.connected && !root.service.scanning)
        onClicked: if (root.service) root.service.rescan(false, function(ok, result, error) {
          root.report(ok, error, "Rescan started")
        })
      }
    }

    Text {
      visible: root.statusText !== ""
      width: Math.min(root.formWidth, parent.width)
      wrapMode: Text.Wrap
      color: root.statusIsError ? Color.urgent : Color.muted
      font.pixelSize: 12
      text: root.statusText
    }

    KeyboardHelp {
      width: Math.min(root.formWidth, parent.width)
    }
    }
  }
}

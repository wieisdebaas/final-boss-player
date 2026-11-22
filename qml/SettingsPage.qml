pragma ComponentBehavior: Bound
import QtQuick
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

  function report(ok, error, success) {
    statusIsError = !ok
    statusText = ok ? success : String(error || "Request failed")
  }

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

  Column {
    anchors.fill: parent
    anchors.margins: 20
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

    PillButton {
      visible: !!(root.service && !root.service.connected)
      label: "Reconnect"
      onClicked: root.service.reconnect()
    }

    Text {
      color: Color.foreground
      font.pixelSize: 14
      font.bold: true
      text: "Music folders"
    }

    Row {
      spacing: 8
      Rectangle {
        width: 420
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
        spacing: 10
        Text {
          anchors.verticalCenter: parent.verticalCenter
          width: 420
          elide: Text.ElideMiddle
          color: Color.foreground
          font.pixelSize: 13
          text: String(folderRow.modelData)
        }
        PillButton {
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
      spacing: 12
      PillButton {
        filled: false
        label: "Rescan"
        enabled: !!(root.service && root.service.connected && !root.service.scanning)
        onClicked: if (root.service) root.service.rescan(false, function(ok, result, error) {
          root.report(ok, error, "Rescan started")
        })
      }
      Text {
        anchors.verticalCenter: parent.verticalCenter
        color: Color.muted
        font.pixelSize: 12
        text: root.scanText
      }
    }

    Text {
      visible: root.statusText !== ""
      width: parent.width
      wrapMode: Text.Wrap
      color: root.statusIsError ? Color.urgent : Color.muted
      font.pixelSize: 12
      text: root.statusText
    }
  }
}

pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons

// Keyboard reference shown at the bottom of Settings.
Column {
  id: root

  spacing: 14

  readonly property var groups: [
    {
      title: "Playback",
      rows: [
        { keys: "Space", action: "Play or pause" },
        { keys: "Ctrl+Left, Ctrl+P", action: "Previous track" },
        { keys: "Ctrl+Right, Ctrl+N", action: "Next track" },
        { keys: "Shift+Left", action: "Seek back 5 seconds" },
        { keys: "Shift+Right", action: "Seek forward 5 seconds" },
        { keys: "Ctrl+Up", action: "Volume up" },
        { keys: "Ctrl+Down", action: "Volume down" },
        { keys: "Ctrl+M", action: "Mute" },
        { keys: "Ctrl+S", action: "Toggle shuffle" },
        { keys: "Ctrl+F", action: "Open Search and select the query" },
        { keys: "Escape", action: "Close the player" }
      ]
    },
    {
      title: "Sidebar and lists",
      rows: [
        { keys: "Tab", action: "Switch the highlight between the sidebar and the list" },
        { keys: "Shift+Tab", action: "Switch the highlight the other way" },
        { keys: "Up / Down", action: "Previous or next row" },
        { keys: "Left / Right", action: "Previous or next cover, while tile view is on" },
        { keys: "Page Up", action: "Jump up one screen" },
        { keys: "Page Down", action: "Jump down one screen" },
        { keys: "Ctrl+Home", action: "First item in the highlighted menu or list" },
        { keys: "Ctrl+End", action: "Last item in the highlighted menu or list" },
        { keys: "Enter", action: "Open the menu page, play the track, or open the album or artist" }
      ]
    }
  ]

  Text {
    width: root.width
    color: Color.foreground
    font.pixelSize: 14
    font.bold: true
    text: "Keyboard"
  }

  Text {
    width: root.width
    wrapMode: Text.Wrap
    color: Color.muted
    font.pixelSize: 12
    lineHeight: 1.35
    text: "Playback keys work anywhere in the window. Tab and Shift+Tab move one highlight between the sidebar and the list on the page. Up and Down move one row. In the cover grid, Left and Right move across that row. Page Up and Page Down jump one screen. Ctrl+Home and Ctrl+End jump to the first and last item. Enter opens the highlighted menu page, plays the highlighted track, or opens the highlighted album or artist. With the highlight on this page, Up, Down, Page Up, Page Down, Ctrl+Home, and Ctrl+End scroll this text. While the cursor is in Search or the folder path, the arrow keys edit the text. Tab leaves the field for the list, and Shift+Tab leaves it for the sidebar. The highlight stays where the last key put it."
  }

  Repeater {
    model: root.groups
    delegate: Column {
      id: group
      required property var modelData
      width: root.width
      spacing: 8

      Text {
        color: Color.foreground
        font.pixelSize: 13
        font.bold: true
        text: group.modelData.title
      }

      Repeater {
        model: group.modelData.rows
        delegate: Item {
          id: line
          required property var modelData
          width: group.width
          height: Math.max(24, actionText.implicitHeight)

          Rectangle {
            id: keyChip
            width: Math.min(240, Math.max(28, keyText.implicitWidth + 14))
            height: 24
            radius: 4
            color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.08)
            border.width: 1
            border.color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.2)

            Text {
              id: keyText
              anchors.centerIn: parent
              color: Color.foreground
              font.pixelSize: 12
              font.family: "monospace"
              text: line.modelData.keys
            }
          }

          Text {
            id: actionText
            anchors.left: parent.left
            anchors.leftMargin: 252
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            wrapMode: Text.Wrap
            color: Color.foreground
            font.pixelSize: 13
            text: line.modelData.action
          }
        }
      }
    }
  }
}

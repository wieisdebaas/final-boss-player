pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import Quickshell.Io

// Line-delimited JSON client for finalbossd (protocol v1).
//
//   request : {"v":1,"id":N,"cmd":"query", ...fields}\n
//   response: {"type":"response","id":N,"ok":true,"result":...}
//             {"type":"response","id":N,"ok":false,"error":{"code","message"}}
//   event   : {"type":"event","event":"state_changed","state":{...}}
//
// The daemon pushes a state snapshot on connect and on every change. The
// socket is retried with a short backoff while `wanted` and not connected.
Item {
  id: root

  visible: false
  width: 0
  height: 0

  property bool wanted: false
  readonly property var activeSocket: socketLoader.item
  readonly property bool connected: !!(activeSocket && activeSocket.connected)
  property var lastState: null
  property int nextId: 1
  property var pending: ({})
  property int reconnectAttempt: 0

  readonly property string socketPath: {
    var runtime = Quickshell.env("XDG_RUNTIME_DIR")
    if (runtime) return String(runtime) + "/finalboss/backend.sock"
    return String(Quickshell.env("HOME") || "") + "/.local/state/finalboss/backend.sock"
  }

  signal stateReceived(var state)

  function resetPending(reason) {
    var waiters = pending
    pending = ({})
    var message = String(reason || "finalbossd is unavailable")
    for (var id in waiters) {
      var callback = waiters[id]
      if (typeof callback === "function") callback(false, null, message)
    }
  }

  // send("query", { kind: "tracks", limit: 100 }, function(ok, result, error) {})
  function send(command, fields, callback) {
    var socket = activeSocket
    if (!socket || !socket.connected) {
      if (typeof callback === "function")
        callback(false, null, "finalbossd is not running")
      return 0
    }
    var id = nextId++
    var payload = { v: 1, id: id, cmd: String(command || "") }
    var extra = fields || ({})
    for (var key in extra) payload[key] = extra[key]
    var next = ({})
    for (var existing in pending) next[existing] = pending[existing]
    next[String(id)] = typeof callback === "function" ? callback : null
    pending = next
    socket.write(JSON.stringify(payload) + "\n")
    socket.flush()
    return id
  }

  function handleLine(line) {
    var message = null
    try { message = JSON.parse(String(line)) } catch (e) { return }
    if (!message || typeof message !== "object") return

    if (message.type === "event") {
      if (message.state) {
        lastState = message.state
        stateReceived(message.state)
      }
      return
    }
    if (message.type !== "response") return

    var id = String(message.id)
    var callback = pending[id]
    if (callback === undefined) return
    var next = ({})
    for (var key in pending) if (key !== id) next[key] = pending[key]
    pending = next
    if (typeof callback !== "function") return
    if (message.ok === true) {
      callback(true, message.result === undefined ? null : message.result, "")
    } else {
      var error = message.error || ({})
      callback(false, null, String(error.message || "Command failed"))
    }
  }

  onWantedChanged: {
    if (wanted) return
    reconnectTimer.stop()
    socketLoader.active = false
    lastState = null
    reconnectAttempt = 0
    resetPending("finalbossd connection closed")
  }

  onConnectedChanged: {
    if (connected) reconnectAttempt = 0
    else resetPending("finalbossd disconnected")
  }

  Component {
    id: socketComponent
    Socket {
      path: root.socketPath
      connected: true
      parser: SplitParser {
        splitMarker: "\n"
        onRead: function(line) { root.handleLine(line) }
      }
      onConnectionStateChanged: {
        if (connected) {
          root.send("hello", null, null)
          root.send("ping", null, function(ok, result, error) {
            if (!ok && root.activeSocket)
              root.activeSocket.connected = false
          })
        }
      }
    }
  }

  Loader {
    id: socketLoader
    active: false
    sourceComponent: socketComponent
  }

  // A failed connect leaves a dead QLocalSocket inside Quickshell's Socket;
  // re-setting connected=true is a no-op, so each retry builds a new Socket.
  Timer {
    id: reconnectTimer
    interval: Math.min(3000, 250 + root.reconnectAttempt * 250)
    repeat: true
    triggeredOnStart: true
    running: root.wanted && !root.connected
    onTriggered: {
      root.reconnectAttempt = Math.min(12, root.reconnectAttempt + 1)
      socketLoader.active = false
      socketLoader.active = true
    }
  }
}

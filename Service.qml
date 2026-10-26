import QtQuick
import Quickshell
import Quickshell.Io
import "Model.js" as Model

Item {
  id: root
  visible: false

  property var settings: ({})

  property bool ready: false
  property string hostname: "System"
  property int uptimeSec: 0

  property var cpu: Model.defaultCpu()
  property var ram: Model.defaultRam()
  property var gpu: Model.defaultGpu()
  property var network: Model.defaultNetwork()
  property var topProcesses: []

  property var cpuHistory: []
  property var ramHistory: []
  property var gpuHistory: []
  property var netRxHistory: []

  readonly property int historyMax: 24

  readonly property int refreshIntervalSec: {
    var v = Number(settings && settings.refreshIntervalSec !== undefined ? settings.refreshIntervalSec : 2)
    if (!isFinite(v) || v < 1) return 2
    return Math.min(30, Math.round(v))
  }

  readonly property string pluginDir: {
    var path = Qt.resolvedUrl(".").toString()
    if (path.indexOf("file://") === 0) path = path.slice(7)
    if (path.length > 1 && path.charAt(path.length - 1) === "/")
      path = path.slice(0, path.length - 1)
    return path
  }

  readonly property string collectorPath: pluginDir + "/collect.py"

  function pushHistory(arr, value) {
    var next = arr ? arr.slice() : []
    next.push(Number(value) || 0)
    if (next.length > historyMax) next = next.slice(next.length - historyMax)
    return next
  }

  function applySnapshot(raw) {
    var snap = Model.parseSnapshot(raw)
    if (!snap.ok) return

    ready = true
    hostname = snap.hostname
    uptimeSec = snap.uptimeSec
    cpu = snap.cpu
    ram = snap.ram
    gpu = snap.gpu
    network = snap.network
    topProcesses = snap.topProcesses

    cpuHistory = pushHistory(cpuHistory, snap.cpu.percent)
    ramHistory = pushHistory(ramHistory, snap.ram.percent)
    gpuHistory = pushHistory(gpuHistory, snap.gpu.percent)
    netRxHistory = pushHistory(netRxHistory, snap.network.rxBytesPerSec)
  }

  function refresh() {
    if (!oneshotProcess.running) {
      oneshotProcess.running = true
    }
  }

  onRefreshIntervalSecChanged: {
    if (streamProcess.running) {
      streamProcess.running = false
    }
    restartTimer.restart()
  }

  Timer {
    id: restartTimer
    interval: 250
    repeat: false
    onTriggered: {
      if (!streamProcess.running) {
        streamProcess.running = true
      }
    }
  }

  Process {
    id: streamProcess
    command: ["python3", "-u", root.collectorPath, "--watch", String(root.refreshIntervalSec)]
    running: true
    stdout: SplitParser {
      onRead: function(data) {
        root.applySnapshot(data)
      }
    }
    onExited: {
      restartTimer.restart()
    }
  }

  Process {
    id: oneshotProcess
    command: ["python3", root.collectorPath]
    running: false
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.applySnapshot(text)
    }
  }
}

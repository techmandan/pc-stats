import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "Model.js" as Model

Panel {
  id: root
  moduleName: "pc-stats"
  ipcTarget: "pc-stats"

  readonly property bool vertical: bar ? bar.vertical : false
  readonly property int barSize: bar ? bar.barSize : Style.bar.sizeHorizontal
  readonly property color foreground: bar ? bar.foreground : Color.foreground
  readonly property color barTextColor: bar ? bar.barForeground : Color.foreground
  readonly property color urgent: bar ? bar.urgent : Color.urgent
  readonly property color dim: Qt.darker(foreground, 1.55)
  readonly property color barDim: Qt.darker(barTextColor, 1.45)
  readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family

  readonly property bool showCpu: setting("showCpu", true) !== false
  readonly property bool showRam: setting("showRam", true) !== false
  readonly property bool showGpu: setting("showGpu", true) !== false
  readonly property bool showNetwork: setting("showNetwork", true) !== false
  readonly property string ramFormat: String(setting("ramFormat", "percent") || "percent")
  readonly property string networkFormat: String(setting("networkFormat", "both") || "both")
  readonly property bool showTempOnBar: setting("showTempOnBar", false) === true

  readonly property bool cpuHigh: service.cpu.percent >= 90 || (service.cpu.tempC !== null && service.cpu.tempC >= 85)
  readonly property bool ramHigh: service.ram.percent >= 90
  readonly property bool gpuHigh: service.gpu.percent >= 95 || (service.gpu.tempC !== null && service.gpu.tempC >= 90)

  readonly property real openPanelIndicatorWidth: root.vertical
    ? 0
    : Math.max(Style.space(16), metricsRow.implicitWidth)

  function updateSetting(key, value) {
    var next = { id: root.moduleName }
    if (root.settings) {
      for (var k in root.settings) {
        if (k !== "id") next[k] = root.settings[k]
      }
    }
    next[key] = value
    root.settings = next
    if (root.bar && root.bar.shell && typeof root.bar.shell.updateEntryInline === "function") {
      root.bar.shell.updateEntryInline(root.moduleName, next)
    }
  }

  function cycleRamFormat() {
    updateSetting("ramFormat", ramFormat === "gb" ? "percent" : "gb")
  }

  function cycleNetworkFormat() {
    var order = ["both", "down", "total"]
    var idx = order.indexOf(networkFormat)
    var next = order[(idx + 1) % order.length]
    updateSetting("networkFormat", next)
  }

  function launchBtop() {
    root.close()
    if (root.bar) {
      root.bar.run("omarchy-launch-or-focus-tui btop")
    }
  }

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  onOpenedChanged: {
    if (opened) {
      if (panelFlick) panelFlick.contentY = 0
      service.refresh()
      Qt.callLater(function () { keyCatcher.forceActiveFocus() })
    }
  }

  Service {
    id: service
    settings: root.settings
  }

  WidgetButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    labelVisible: false
    hasVisualContent: true
    fixedWidth: root.vertical
      ? root.barSize
      : Math.max(Style.bar.iconSlot, metricsRow.implicitWidth + Style.space(16))
    fixedHeight: root.vertical ? Style.bar.iconSlot : root.barSize

    tooltipText: {
      if (!service.ready) return "PC Stats (Collecting telemetry…)"
      var parts = []
      var cpuStr = "CPU " + Model.formatPercent(service.cpu.percent)
      if (service.cpu.tempC !== null) cpuStr += " (" + Model.formatTemp(service.cpu.tempC) + ")"
      parts.push(cpuStr)

      parts.push("RAM " + Model.formatBytesGiB(service.ram.usedBytes, 1) + " / " + Model.formatBytesGiB(service.ram.totalBytes, 1) + " (" + Model.formatPercent(service.ram.percent) + ")")

      if (service.gpu.available) {
        var gpuStr = "GPU " + Model.formatPercent(service.gpu.percent)
        if (service.gpu.tempC !== null) gpuStr += " (" + Model.formatTemp(service.gpu.tempC) + ")"
        parts.push(gpuStr)
      }

      if (service.network.connected) {
        parts.push("NET ↓" + Model.formatRateDetailed(service.network.rxBytesPerSec) + " ↑" + Model.formatRateDetailed(service.network.txBytesPerSec))
      } else {
        parts.push("NET Offline")
      }
      return parts.join("  ·  ")
    }

    onPressed: function (b) {
      if (b === Qt.RightButton) root.cycleRamFormat()
      else if (b === Qt.MiddleButton) root.launchBtop()
      else root.toggle()
    }

    // Horizontal bar layout: CPU · RAM · GPU · Network
    Row {
      id: metricsRow
      visible: !root.vertical
      anchors.centerIn: parent
      spacing: Style.space(10)

      // CPU Stat
      Row {
        visible: root.showCpu
        spacing: Style.space(4)
        anchors.verticalCenter: parent.verticalCenter

        Text {
          textFormat: Text.PlainText
          text: "󰍛"
          color: root.cpuHigh ? root.urgent : root.barDim
          font.family: root.fontFamily
          font.pixelSize: Style.font.body
          anchors.verticalCenter: parent.verticalCenter
        }

        Text {
          textFormat: Text.PlainText
          text: {
            var s = Model.formatPercent(service.cpu.percent)
            if (root.showTempOnBar && service.cpu.tempC !== null) {
              s += " " + Model.formatTemp(service.cpu.tempC)
            }
            return s
          }
          color: root.cpuHigh ? root.urgent : root.barTextColor
          font.family: root.fontFamily
          font.pixelSize: Style.font.body
          anchors.verticalCenter: parent.verticalCenter
        }
      }

      // RAM Stat
      Row {
        visible: root.showRam
        spacing: Style.space(4)
        anchors.verticalCenter: parent.verticalCenter

        Text {
          textFormat: Text.PlainText
          text: "󰘚"
          color: root.ramHigh ? root.urgent : root.barDim
          font.family: root.fontFamily
          font.pixelSize: Style.font.body
          anchors.verticalCenter: parent.verticalCenter
        }

        Text {
          textFormat: Text.PlainText
          text: Model.formatRamBar(service.ram, root.ramFormat)
          color: root.ramHigh ? root.urgent : root.barTextColor
          font.family: root.fontFamily
          font.pixelSize: Style.font.body
          anchors.verticalCenter: parent.verticalCenter
        }
      }

      // GPU Stat
      Row {
        visible: root.showGpu && (service.gpu.available || !service.ready)
        spacing: Style.space(4)
        anchors.verticalCenter: parent.verticalCenter

        Text {
          textFormat: Text.PlainText
          text: "󰢮"
          color: root.gpuHigh ? root.urgent : root.barDim
          font.family: root.fontFamily
          font.pixelSize: Style.font.body
          anchors.verticalCenter: parent.verticalCenter
        }

        Text {
          textFormat: Text.PlainText
          text: {
            var s = Model.formatPercent(service.gpu.percent)
            if (root.showTempOnBar && service.gpu.tempC !== null) {
              s += " " + Model.formatTemp(service.gpu.tempC)
            }
            return s
          }
          color: root.gpuHigh ? root.urgent : root.barTextColor
          font.family: root.fontFamily
          font.pixelSize: Style.font.body
          anchors.verticalCenter: parent.verticalCenter
        }
      }

      // Network Stat
      Row {
        visible: root.showNetwork
        spacing: Style.space(4)
        anchors.verticalCenter: parent.verticalCenter

        Text {
          textFormat: Text.PlainText
          text: !service.network.connected
            ? "󰤮"
            : (service.network.kind === "wifi" ? "󰤨" : "󰈀")
          color: !service.network.connected ? root.urgent : root.barDim
          font.family: root.fontFamily
          font.pixelSize: Style.font.body
          anchors.verticalCenter: parent.verticalCenter
        }

        Text {
          textFormat: Text.PlainText
          text: Model.formatNetworkBar(service.network, root.networkFormat)
          color: !service.network.connected ? root.urgent : root.barTextColor
          font.family: root.fontFamily
          font.pixelSize: Style.font.body
          anchors.verticalCenter: parent.verticalCenter
        }
      }

      // Fallback icon if user hides all 4 metrics
      PcStatsIcon {
        visible: !root.showCpu && !root.showRam && !root.showGpu && !root.showNetwork
        iconSize: Style.space(16)
        color: root.barTextColor
        anchors.verticalCenter: parent.verticalCenter
      }
    }

    // Compact icon for vertical bar orientation
    PcStatsIcon {
      visible: root.vertical
      anchors.centerIn: parent
      iconSize: Style.space(16)
      color: (root.cpuHigh || root.ramHigh || root.gpuHigh) ? root.urgent : root.barTextColor
    }
  }

  KeyboardPanel {
    id: panel
    anchorItem: button
    owner: root
    bar: root.bar
    open: root.opened
    focusTarget: keyCatcher
    contentWidth: panel.fittedContentWidth(Style.space(400))
    contentHeight: panel.fittedContentHeight(column.implicitHeight + Style.space(20), Style.space(820))

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      onCloseRequested: root.close()
      onActivateRequested: root.launchBtop()
      onTextKey: function (t) {
        var key = String(t).toLowerCase()
        if (key === "r") service.refresh()
        else if (key === "b") root.launchBtop()
        else if (key === "m") root.cycleRamFormat()
        else if (key === "n") root.cycleNetworkFormat()
        else if (key === "t") root.updateSetting("showTempOnBar", !root.showTempOnBar)
        else if (key === "1") root.updateSetting("showCpu", !root.showCpu)
        else if (key === "2") root.updateSetting("showRam", !root.showRam)
        else if (key === "3") root.updateSetting("showGpu", !root.showGpu)
        else if (key === "4") root.updateSetting("showNetwork", !root.showNetwork)
      }

      Flickable {
        id: panelFlick
        anchors.fill: parent
        contentWidth: width
        contentHeight: column.implicitHeight + Style.space(12)
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.VerticalFlick
        interactive: contentHeight > height
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

        Column {
          id: column
          width: panelFlick.width
          spacing: Style.space(12)

          PanelHero {
            width: parent.width
            title: service.hostname
            meta: "UP " + Model.formatUptime(service.uptimeSec) + " · LOAD " + service.cpu.load1m.toFixed(2)
            detail: service.gpu.available && service.gpu.powerWatts !== null
              ? Math.round(service.gpu.powerWatts) + "W GPU"
              : Model.formatPercent(service.cpu.percent) + " CPU"
            foreground: root.foreground
            fontFamily: root.fontFamily
            iconComponent: Component {
              Item {
                width: Style.font.display * 1.25
                height: Style.font.display * 1.25
                PcStatsIcon {
                  anchors.centerIn: parent
                  iconSize: Style.font.display * 1.2
                  color: Color.accent
                }
              }
            }
            trailingControl: Component {
              PanelActionButton {
                iconText: "󰄨"
                tooltipText: "Open btop (B)"
                foreground: root.foreground
                fontFamily: root.fontFamily
                bordered: true
                onClicked: root.launchBtop()
              }
            }
          }

          PanelSeparator { foreground: root.foreground }

          // ==================== PROCESSOR ====================
          Column {
            width: parent.width
            spacing: Style.space(8)

            RowLayout {
              width: parent.width
              PanelSectionHeader {
                text: "PROCESSOR"
                foreground: root.foreground
                fontFamily: root.fontFamily
                Layout.fillWidth: true
              }
              Text {
                textFormat: Text.PlainText
                text: {
                  var p = []
                  if (service.cpu.freqMhz > 0) p.push(Model.formatFreq(service.cpu.freqMhz))
                  if (service.cpu.tempC !== null) p.push(Model.formatTemp(service.cpu.tempC))
                  return p.join(" · ")
                }
                color: root.cpuHigh ? root.urgent : root.dim
                font.family: root.fontFamily
                font.pixelSize: Style.font.caption
                font.bold: true
              }
            }

            RowLayout {
              width: parent.width
              spacing: Style.space(8)

              Text {
                textFormat: Text.PlainText
                text: service.cpu.model
                color: root.foreground
                font.family: root.fontFamily
                font.pixelSize: Style.font.bodySmall
                elide: Text.ElideRight
                Layout.fillWidth: true
              }

              Text {
                textFormat: Text.PlainText
                text: service.cpu.cores + "C / " + service.cpu.threads + "T"
                color: root.dim
                font.family: root.fontFamily
                font.pixelSize: Style.font.caption
              }
            }

            MetricBarRow {
              width: parent.width
              label: "Total"
              fraction: Math.max(0, Math.min(1, service.cpu.percent / 100.0))
              valueText: Model.formatPercent(service.cpu.percent)
              subText: "Load " + service.cpu.load1m.toFixed(2) + " / " + service.cpu.load5m.toFixed(2)
              isUrgent: root.cpuHigh
            }

            // Per-thread mini equalizer bars
            Item {
              visible: service.cpu.perCore && service.cpu.perCore.length > 0
              width: parent.width
              height: Style.space(24)

              Row {
                anchors.fill: parent
                spacing: Style.space(3)

                Repeater {
                  model: service.cpu.perCore || []

                  Item {
                    required property var modelData
                    required property int index
                    readonly property int count: (service.cpu.perCore || []).length
                    width: Math.max(4, Math.floor((parent.width - Math.max(0, count - 1) * Style.space(3)) / Math.max(1, count)))
                    height: parent.height

                    Rectangle {
                      anchors.fill: parent
                      radius: 2
                      color: Qt.darker(root.foreground, 3.4)
                    }

                    Rectangle {
                      width: parent.width
                      height: Math.max(2, Math.round(parent.height * Math.max(0, Math.min(1, Number(modelData) / 100.0))))
                      anchors.bottom: parent.bottom
                      radius: 2
                      color: Number(modelData) >= 90 ? root.urgent : Color.accent

                      Behavior on height {
                        NumberAnimation { duration: 180; easing.type: Easing.OutCubic }
                      }
                    }
                  }
                }
              }
            }

            Text {
              visible: service.topProcesses && service.topProcesses.length > 0
              width: parent.width
              textFormat: Text.PlainText
              elide: Text.ElideRight
              text: {
                var list = service.topProcesses || []
                var out = []
                for (var i = 0; i < list.length; i++) {
                  out.push(list[i].name + " (" + Math.round(list[i].cpu) + "%)")
                }
                return "Top: " + out.join(" · ")
              }
              color: root.dim
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
            }
          }

          PanelSeparator { foreground: root.foreground }

          // ==================== MEMORY ====================
          Column {
            width: parent.width
            spacing: Style.space(8)

            RowLayout {
              width: parent.width
              PanelSectionHeader {
                text: "MEMORY"
                foreground: root.foreground
                fontFamily: root.fontFamily
                Layout.fillWidth: true
              }
              Text {
                textFormat: Text.PlainText
                text: Model.formatBytesGiB(service.ram.usedBytes, 1) + " / " + Model.formatBytesGiB(service.ram.totalBytes, 1)
                color: root.ramHigh ? root.urgent : root.dim
                font.family: root.fontFamily
                font.pixelSize: Style.font.caption
                font.bold: true
              }
            }

            MetricBarRow {
              width: parent.width
              label: "RAM"
              fraction: Math.max(0, Math.min(1, service.ram.percent / 100.0))
              valueText: Model.formatPercent(service.ram.percent)
              subText: Model.formatBytesGiB(service.ram.availableBytes, 1) + " free"
              isUrgent: root.ramHigh
            }

            MetricBarRow {
              visible: service.ram.swapTotalBytes > 0
              width: parent.width
              label: "Swap"
              fraction: Math.max(0, Math.min(1, service.ram.swapPercent / 100.0))
              valueText: Model.formatPercent(service.ram.swapPercent)
              subText: Model.formatBytesAuto(service.ram.swapUsedBytes) + " / " + Model.formatBytesGiB(service.ram.swapTotalBytes, 0)
              isUrgent: service.ram.swapPercent >= 85
            }

            Text {
              width: parent.width
              textFormat: Text.PlainText
              text: "Cached & Buffers: " + Model.formatBytesGiB(service.ram.cachedBytes, 1)
              color: root.dim
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
            }
          }

          PanelSeparator {
            visible: service.gpu.available
            foreground: root.foreground
          }

          // ==================== GRAPHICS ====================
          Column {
            visible: service.gpu.available
            width: parent.width
            spacing: Style.space(8)

            RowLayout {
              width: parent.width
              PanelSectionHeader {
                text: "GRAPHICS"
                foreground: root.foreground
                fontFamily: root.fontFamily
                Layout.fillWidth: true
              }
              Text {
                textFormat: Text.PlainText
                text: {
                  var p = []
                  if (service.gpu.clockMhz !== null && service.gpu.clockMhz > 0) p.push(service.gpu.clockMhz + " MHz")
                  if (service.gpu.powerWatts !== null) p.push(Math.round(service.gpu.powerWatts) + "W")
                  if (service.gpu.tempC !== null) p.push(Model.formatTemp(service.gpu.tempC))
                  return p.join(" · ")
                }
                color: root.gpuHigh ? root.urgent : root.dim
                font.family: root.fontFamily
                font.pixelSize: Style.font.caption
                font.bold: true
              }
            }

            Text {
              width: parent.width
              textFormat: Text.PlainText
              text: service.gpu.name
              color: root.foreground
              font.family: root.fontFamily
              font.pixelSize: Style.font.bodySmall
              elide: Text.ElideRight
            }

            MetricBarRow {
              width: parent.width
              label: "Core"
              fraction: Math.max(0, Math.min(1, service.gpu.percent / 100.0))
              valueText: Model.formatPercent(service.gpu.percent)
              subText: service.gpu.hotspotTempC !== null
                ? "Hotspot " + Model.formatTemp(service.gpu.hotspotTempC)
                : (service.gpu.tempC !== null ? "Temp " + Model.formatTemp(service.gpu.tempC) : "")
              isUrgent: root.gpuHigh
            }

            MetricBarRow {
              visible: service.gpu.vramTotalBytes > 0
              width: parent.width
              label: "VRAM"
              fraction: Math.max(0, Math.min(1, service.gpu.vramPercent / 100.0))
              valueText: Model.formatPercent(service.gpu.vramPercent)
              subText: Model.formatBytesGiB(service.gpu.vramUsedBytes, 1) + " / " + Model.formatBytesGiB(service.gpu.vramTotalBytes, 0)
              isUrgent: service.gpu.vramPercent >= 92
            }

            Text {
              width: parent.width
              textFormat: Text.PlainText
              elide: Text.ElideRight
              text: {
                var items = []
                if (service.gpu.tempC !== null) items.push("Edge " + Model.formatTemp(service.gpu.tempC))
                if (service.gpu.memTempC !== null) items.push("Mem " + Model.formatTemp(service.gpu.memTempC))
                if (service.gpu.powerWatts !== null) {
                  var pw = Math.round(service.gpu.powerWatts) + "W"
                  if (service.gpu.powerCapWatts) pw += " / " + Math.round(service.gpu.powerCapWatts) + "W"
                  items.push("Power " + pw)
                }
                if (service.gpu.fanRpm !== null) {
                  items.push(service.gpu.fanRpm > 0 ? ("Fan " + service.gpu.fanRpm + " RPM") : "Fan Zero-RPM")
                }
                return items.join(" · ")
              }
              color: root.dim
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
            }
          }

          PanelSeparator { foreground: root.foreground }

          // ==================== NETWORK ====================
          Column {
            width: parent.width
            spacing: Style.space(8)

            RowLayout {
              width: parent.width
              PanelSectionHeader {
                text: "NETWORK"
                foreground: root.foreground
                fontFamily: root.fontFamily
                Layout.fillWidth: true
              }
              Text {
                textFormat: Text.PlainText
                text: service.network.connected
                  ? (service.network.ip || service.network.interface)
                  : "Disconnected"
                color: service.network.connected ? root.dim : root.urgent
                font.family: root.fontFamily
                font.pixelSize: Style.font.caption
                font.bold: true
              }
            }

            RowLayout {
              width: parent.width
              spacing: Style.space(8)

              Text {
                textFormat: Text.PlainText
                text: {
                  if (!service.network.connected) return "No active network interface"
                  var kind = service.network.kind === "wifi" ? "Wi-Fi" : "Ethernet"
                  var s = service.network.interface + " (" + kind
                  var spd = Model.formatLinkSpeed(service.network.linkSpeedMbps)
                  if (spd) s += " · " + spd
                  return s + ")"
                }
                color: root.foreground
                font.family: root.fontFamily
                font.pixelSize: Style.font.bodySmall
                elide: Text.ElideRight
                Layout.fillWidth: true
              }
            }

            RowLayout {
              width: parent.width
              spacing: Style.space(8)

              NetCard {
                Layout.fillWidth: true
                title: "↓ DOWNLOAD"
                rateText: Model.formatRateDetailed(service.network.rxBytesPerSec)
                totalText: "Total " + Model.formatBytesAuto(service.network.rxTotalBytes)
              }

              NetCard {
                Layout.fillWidth: true
                title: "↑ UPLOAD"
                rateText: Model.formatRateDetailed(service.network.txBytesPerSec)
                totalText: "Total " + Model.formatBytesAuto(service.network.txTotalBytes)
              }
            }
          }

          PanelSeparator { foreground: root.foreground }

          // ==================== BAR DISPLAY TOGGLES ====================
          Column {
            width: parent.width
            spacing: Style.space(8)

            PanelSectionHeader {
              text: "TOP BAR DISPLAY"
              foreground: root.foreground
              fontFamily: root.fontFamily
            }

            Flow {
              width: parent.width
              spacing: Style.space(6)

              BarToggleChip {
                label: "CPU"
                active: root.showCpu
                onClicked: root.updateSetting("showCpu", !root.showCpu)
              }

              BarToggleChip {
                label: "RAM"
                active: root.showRam
                onClicked: root.updateSetting("showRam", !root.showRam)
              }

              BarToggleChip {
                label: "GPU"
                active: root.showGpu
                onClicked: root.updateSetting("showGpu", !root.showGpu)
              }

              BarToggleChip {
                label: "Network"
                active: root.showNetwork
                onClicked: root.updateSetting("showNetwork", !root.showNetwork)
              }

              BarToggleChip {
                label: "RAM: " + (root.ramFormat === "gb" ? "GB" : "%")
                active: root.ramFormat === "gb"
                onClicked: root.cycleRamFormat()
              }

              BarToggleChip {
                label: "Temps"
                active: root.showTempOnBar
                onClicked: root.updateSetting("showTempOnBar", !root.showTempOnBar)
              }
            }
          }
        }
      }
    }
  }

  component MetricBarRow: RowLayout {
    id: metricRow
    property string label: ""
    property real fraction: 0
    property string valueText: ""
    property string subText: ""
    property bool isUrgent: false

    spacing: Style.space(8)

    Text {
      textFormat: Text.PlainText
      text: metricRow.label
      color: root.foreground
      opacity: 0.78
      font.family: root.fontFamily
      font.pixelSize: Style.font.bodySmall
      Layout.preferredWidth: Style.space(42)
    }

    Rectangle {
      id: track
      Layout.fillWidth: true
      Layout.alignment: Qt.AlignVCenter
      implicitHeight: Style.space(6)
      radius: height / 2
      color: Qt.darker(root.foreground, 3.2)

      Rectangle {
        width: Math.max(0, Math.min(track.width, track.width * metricRow.fraction))
        height: parent.height
        radius: parent.radius
        color: metricRow.isUrgent ? root.urgent : Color.accent

        Behavior on width {
          NumberAnimation { duration: 180; easing.type: Easing.OutCubic }
        }
      }
    }

    Text {
      textFormat: Text.PlainText
      text: metricRow.valueText
      color: metricRow.isUrgent ? root.urgent : root.foreground
      font.family: root.fontFamily
      font.pixelSize: Style.font.bodySmall
      font.bold: true
      horizontalAlignment: Text.AlignRight
      Layout.preferredWidth: Style.space(38)
    }

    Text {
      visible: metricRow.subText !== ""
      textFormat: Text.PlainText
      text: metricRow.subText
      color: root.dim
      font.family: root.fontFamily
      font.pixelSize: Style.font.caption
      horizontalAlignment: Text.AlignRight
      elide: Text.ElideRight
      Layout.preferredWidth: Style.space(102)
    }
  }

  component NetCard: BorderSurface {
    id: netCard
    property string title: ""
    property string rateText: ""
    property string totalText: ""

    implicitHeight: cardCol.implicitHeight + Style.space(16)
    color: "transparent"
    borderSpec: Border.controlSpec("normal", root.foreground, Color.accent)
    radius: Style.cornerRadius

    Column {
      id: cardCol
      anchors.left: parent.left
      anchors.right: parent.right
      anchors.verticalCenter: parent.verticalCenter
      anchors.leftMargin: Style.space(10)
      anchors.rightMargin: Style.space(10)
      spacing: Style.space(2)

      Text {
        textFormat: Text.PlainText
        text: netCard.title
        color: root.dim
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        font.bold: true
      }

      Text {
        textFormat: Text.PlainText
        text: netCard.rateText
        color: root.foreground
        font.family: root.fontFamily
        font.pixelSize: Style.font.body
        font.bold: true
      }

      Text {
        textFormat: Text.PlainText
        text: netCard.totalText
        color: root.dim
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
      }
    }
  }

  component BarToggleChip: BorderSurface {
    id: chip
    property string label: ""
    property bool active: false
    signal clicked()

    implicitWidth: chipText.implicitWidth + Style.space(16)
    implicitHeight: chipText.implicitHeight + Style.space(8)
    radius: Style.cornerRadius
    color: chipMouse.containsMouse
      ? Style.hoverFillFor(root.foreground, Color.accent)
      : (active ? Qt.rgba(Color.accent.r, Color.accent.g, Color.accent.b, 0.14) : "transparent")
    borderSpec: active
      ? Border.controlSpec("focus", Color.accent, Color.accent)
      : Border.controlSpec("normal", root.foreground, Color.accent)

    Text {
      id: chipText
      textFormat: Text.PlainText
      anchors.centerIn: parent
      text: chip.label
      color: chip.active ? root.foreground : root.dim
      font.family: root.fontFamily
      font.pixelSize: Style.font.caption
      font.bold: chip.active
    }

    MouseArea {
      id: chipMouse
      anchors.fill: parent
      hoverEnabled: true
      cursorShape: Qt.PointingHandCursor
      onClicked: chip.clicked()
    }
  }
}

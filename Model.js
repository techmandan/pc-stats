function defaultCpu() {
  return {
    model: "Processor",
    cores: 1,
    threads: 1,
    percent: 0,
    perCore: [],
    tempC: null,
    freqMhz: 0,
    maxFreqMhz: 0,
    load1m: 0,
    load5m: 0,
    load15m: 0
  }
}

function defaultRam() {
  return {
    usedBytes: 0,
    availableBytes: 0,
    totalBytes: 0,
    cachedBytes: 0,
    percent: 0,
    swapUsedBytes: 0,
    swapTotalBytes: 0,
    swapPercent: 0
  }
}

function defaultGpu() {
  return {
    available: false,
    vendor: "",
    name: "GPU",
    percent: 0,
    vramUsedBytes: 0,
    vramTotalBytes: 0,
    vramPercent: 0,
    tempC: null,
    hotspotTempC: null,
    memTempC: null,
    powerWatts: null,
    powerCapWatts: null,
    clockMhz: null,
    memClockMhz: null,
    fanRpm: null
  }
}

function defaultNetwork() {
  return {
    interface: "offline",
    connected: false,
    kind: "disconnected",
    ip: "",
    linkSpeedMbps: null,
    rxBytesPerSec: 0,
    txBytesPerSec: 0,
    rxTotalBytes: 0,
    txTotalBytes: 0
  }
}

function parseSnapshot(raw) {
  var text = String(raw || "").trim()
  if (!text) return { ok: false }
  try {
    var obj = JSON.parse(text)
    if (!obj || typeof obj !== "object") return { ok: false }
    return {
      ok: obj.ok !== false,
      timestamp: Number(obj.timestamp || 0),
      hostname: String(obj.hostname || "System"),
      uptimeSec: Number(obj.uptimeSec || 0),
      cpu: Object.assign(defaultCpu(), obj.cpu || {}),
      ram: Object.assign(defaultRam(), obj.ram || {}),
      gpu: Object.assign(defaultGpu(), obj.gpu || {}),
      network: Object.assign(defaultNetwork(), obj.network || {}),
      topProcesses: Array.isArray(obj.topProcesses) ? obj.topProcesses : []
    }
  } catch (e) {
    return { ok: false }
  }
}

function formatPercent(value) {
  var v = Number(value)
  if (!isFinite(v) || v < 0) v = 0
  return Math.round(v) + "%"
}

function formatTemp(tempC) {
  if (tempC === null || tempC === undefined || !isFinite(Number(tempC))) return ""
  return Math.round(Number(tempC)) + "°C"
}

function formatBytesGiB(bytes, decimals) {
  var b = Number(bytes)
  if (!isFinite(b) || b <= 0) return "0.0 GB"
  var gib = b / (1024 * 1024 * 1024)
  var d = decimals !== undefined ? decimals : (gib >= 10 ? 1 : 2)
  return gib.toFixed(d) + " GB"
}

function formatBytesAuto(bytes) {
  var b = Number(bytes)
  if (!isFinite(b) || b <= 0) return "0 MB"
  var gib = b / (1024 * 1024 * 1024)
  if (gib >= 1.0) return gib.toFixed(gib >= 10 ? 1 : 2) + " GB"
  var mib = b / (1024 * 1024)
  if (mib >= 1.0) return Math.round(mib) + " MB"
  var kib = b / 1024
  return Math.round(kib) + " KB"
}

function formatRamBar(ram, format) {
  var r = ram || defaultRam()
  if (format === "gb") {
    var gib = Number(r.usedBytes || 0) / (1024 * 1024 * 1024)
    return gib.toFixed(1) + "G"
  }
  return formatPercent(r.percent)
}

function formatRateCompact(bytesPerSec) {
  var b = Number(bytesPerSec)
  if (!isFinite(b) || b < 512) return "0K"
  var kib = b / 1024
  if (kib < 1000) return Math.round(kib) + "K"
  var mib = kib / 1024
  if (mib < 100) return mib.toFixed(1) + "M"
  if (mib < 1000) return Math.round(mib) + "M"
  var gib = mib / 1024
  return gib.toFixed(2) + "G"
}

function formatRateDetailed(bytesPerSec) {
  var b = Number(bytesPerSec)
  if (!isFinite(b) || b <= 0) return "0 B/s"
  if (b < 1024) return Math.round(b) + " B/s"
  var kib = b / 1024
  if (kib < 1024) return (kib >= 100 ? Math.round(kib) : kib.toFixed(1)) + " KB/s"
  var mib = kib / 1024
  if (mib < 1024) return mib.toFixed(2) + " MB/s"
  var gib = mib / 1024
  return gib.toFixed(2) + " GB/s"
}

function formatNetworkBar(net, format) {
  var n = net || defaultNetwork()
  if (!n.connected) return "Offline"
  var rx = formatRateCompact(n.rxBytesPerSec)
  var tx = formatRateCompact(n.txBytesPerSec)
  if (format === "down") return "↓" + rx
  if (format === "total") return formatRateCompact((n.rxBytesPerSec || 0) + (n.txBytesPerSec || 0))
  return "↓" + rx + " ↑" + tx
}

function formatFreq(mhz) {
  var m = Number(mhz)
  if (!isFinite(m) || m <= 0) return ""
  if (m >= 1000) return (m / 1000).toFixed(2) + " GHz"
  return Math.round(m) + " MHz"
}

function formatUptime(seconds) {
  var s = Math.max(0, Math.floor(Number(seconds) || 0))
  var days = Math.floor(s / 86400)
  var hours = Math.floor((s % 86400) / 3600)
  var mins = Math.floor((s % 3600) / 60)
  var parts = []
  if (days > 0) parts.push(days + "d")
  if (hours > 0 || days > 0) parts.push(hours + "h")
  parts.push(mins + "m")
  return parts.join(" ")
}

function formatLinkSpeed(mbps) {
  var v = Number(mbps)
  if (!isFinite(v) || v <= 0) return ""
  if (v >= 1000) {
    var g = v / 1000
    return (g % 1 === 0 ? g.toFixed(0) : g.toFixed(1)) + " Gbps"
  }
  return Math.round(v) + " Mbps"
}

if (typeof module !== "undefined") {
  module.exports = {
    defaultCpu: defaultCpu,
    defaultRam: defaultRam,
    defaultGpu: defaultGpu,
    defaultNetwork: defaultNetwork,
    parseSnapshot: parseSnapshot,
    formatPercent: formatPercent,
    formatTemp: formatTemp,
    formatBytesGiB: formatBytesGiB,
    formatBytesAuto: formatBytesAuto,
    formatRamBar: formatRamBar,
    formatRateCompact: formatRateCompact,
    formatRateDetailed: formatRateDetailed,
    formatNetworkBar: formatNetworkBar,
    formatFreq: formatFreq,
    formatUptime: formatUptime,
    formatLinkSpeed: formatLinkSpeed
  }
}

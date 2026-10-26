#!/usr/bin/env python3
"""
PC Stats telemetry collector for Omarchy Shell.
Zero external dependencies (Python 3 standard library only).
Supports one-shot JSON output or continuous streaming (--watch <interval>).
"""

import glob
import json
import os
import re
import signal
import socket
import subprocess
import sys
import time
from pathlib import Path

STATE_DIR = Path(os.environ.get("XDG_RUNTIME_DIR") or f"/tmp/pc-stats-{os.getuid()}") / "pc-stats"
STATE_FILE = STATE_DIR / "state.json"

_STATIC_CACHE = {}


def read_text(path, default=""):
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            return f.read().strip()
    except OSError:
        return default


def read_int(path, default=None):
    val = read_text(path, "")
    if not val:
        return default
    try:
        return int(val, 0)
    except ValueError:
        return default


def get_cpu_static():
    if "cpu_static" in _STATIC_CACHE:
        return _STATIC_CACHE["cpu_static"]

    model_name = "Processor"
    cores_physical = set()
    threads = 0
    try:
        with open("/proc/cpuinfo", "r", encoding="utf-8", errors="replace") as f:
            phys_id = "0"
            core_id = None
            for line in f:
                if ":" not in line:
                    continue
                k, v = [x.strip() for x in line.split(":", 1)]
                if k == "processor":
                    threads += 1
                elif k == "model name" and model_name == "Processor":
                    # Clean redundant suffixes like "Eight-Core Processor" or "(R)"
                    cleaned = re.sub(r"\((?:R|TM|tm)\)", "", v)
                    cleaned = re.sub(r"\b(?:Eight|Six|Four|Twelve|Sixteen|24|32|64)-Core Processor\b", "", cleaned, flags=re.IGNORECASE)
                    cleaned = re.sub(r"\bProcessor\b", "", cleaned, flags=re.IGNORECASE)
                    cleaned = re.sub(r"\s+@.*$", "", cleaned)
                    cleaned = re.sub(r"\s+", " ", cleaned).strip()
                    model_name = cleaned or v
                elif k == "physical id":
                    phys_id = v
                elif k == "core id":
                    core_id = v
                    cores_physical.add((phys_id, core_id))
    except OSError:
        pass

    info = {
        "model": model_name,
        "cores": len(cores_physical) if cores_physical else max(1, threads),
        "threads": max(1, threads),
    }
    _STATIC_CACHE["cpu_static"] = info
    return info


def read_proc_stat():
    cpus = {}
    try:
        with open("/proc/stat", "r", encoding="utf-8") as f:
            for line in f:
                if not line.startswith("cpu"):
                    break
                parts = line.split()
                name = parts[0]
                nums = [int(x) for x in parts[1:]]
                # idle = idle + iowait
                idle = nums[3] + (nums[4] if len(nums) > 4 else 0)
                total = sum(nums)
                cpus[name] = (idle, total)
    except OSError:
        pass
    return cpus


def compute_cpu_usage(prev_cpus, cur_cpus):
    overall = 0.0
    if "cpu" in prev_cpus and "cpu" in cur_cpus:
        p_idle, p_total = prev_cpus["cpu"]
        c_idle, c_total = cur_cpus["cpu"]
        d_total = c_total - p_total
        d_idle = c_idle - p_idle
        if d_total > 0:
            overall = max(0.0, min(100.0, ((d_total - d_idle) / d_total) * 100.0))

    per_core = []
    idx = 0
    while f"cpu{idx}" in cur_cpus:
        key = f"cpu{idx}"
        if key in prev_cpus:
            p_idle, p_total = prev_cpus[key]
            c_idle, c_total = cur_cpus[key]
            d_total = c_total - p_total
            d_idle = c_idle - p_idle
            pct = max(0.0, min(100.0, ((d_total - d_idle) / d_total) * 100.0)) if d_total > 0 else 0.0
            per_core.append(round(pct, 1))
        else:
            per_core.append(0.0)
        idx += 1

    return round(overall, 1), per_core


def get_cpu_freq_mhz():
    freqs = []
    for p in glob.glob("/sys/devices/system/cpu/cpu[0-9]*/cpufreq/scaling_cur_freq"):
        val = read_int(p)
        if val and val > 0:
            freqs.append(val / 1000.0)
    if freqs:
        return round(sum(freqs) / len(freqs), 0), round(max(freqs), 0)

    # Fallback to /proc/cpuinfo
    try:
        with open("/proc/cpuinfo", "r", encoding="utf-8") as f:
            for line in f:
                if line.startswith("cpu MHz"):
                    val = float(line.split(":", 1)[1].strip())
                    freqs.append(val)
    except (OSError, ValueError):
        pass
    if freqs:
        return round(sum(freqs) / len(freqs), 0), round(max(freqs), 0)
    return 0, 0


def get_cpu_temp():
    best_temp = None
    fallback_temp = None

    for hwmon in sorted(glob.glob("/sys/class/hwmon/hwmon*")):
        name = read_text(f"{hwmon}/name")
        if name in ("k10temp", "zenpower", "coretemp", "cpu_thermal", "acpitz"):
            label_map = {}
            for temp_input in sorted(glob.glob(f"{hwmon}/temp*_input")):
                prefix = temp_input[:-6]
                lbl = read_text(f"{prefix}_label").lower()
                raw = read_int(temp_input)
                if raw is None or raw <= 0:
                    continue
                deg = raw / 1000.0
                label_map[lbl] = deg
                if fallback_temp is None:
                    fallback_temp = deg

            for preferred in ("tdie", "tccd1", "package id 0", "core 0", "tctl"):
                if preferred in label_map:
                    best_temp = label_map[preferred]
                    break
            if best_temp is not None:
                break

    res = best_temp if best_temp is not None else fallback_temp
    return round(res, 1) if res is not None else None


def get_memory_stats():
    info = {}
    try:
        with open("/proc/meminfo", "r", encoding="utf-8") as f:
            for line in f:
                if ":" not in line:
                    continue
                k, rest = line.split(":", 1)
                parts = rest.strip().split()
                if parts:
                    info[k] = int(parts[0]) * 1024  # bytes
    except (OSError, ValueError):
        pass

    total = info.get("MemTotal", 0)
    avail = info.get("MemAvailable", info.get("MemFree", 0))
    used = max(0, total - avail)
    cached = info.get("Cached", 0) + info.get("SReclaimable", 0)
    buffers = info.get("Buffers", 0)
    pct = round((used / total) * 100.0, 1) if total > 0 else 0.0

    swap_total = info.get("SwapTotal", 0)
    swap_free = info.get("SwapFree", 0)
    swap_used = max(0, swap_total - swap_free)
    swap_pct = round((swap_used / swap_total) * 100.0, 1) if swap_total > 0 else 0.0

    return {
        "usedBytes": used,
        "availableBytes": avail,
        "totalBytes": total,
        "cachedBytes": cached + buffers,
        "percent": pct,
        "swapUsedBytes": swap_used,
        "swapTotalBytes": swap_total,
        "swapPercent": swap_pct,
    }


def resolve_amd_gpu_name(dev_dir):
    cache_key = f"gpu_name:{dev_dir}"
    if cache_key in _STATIC_CACHE:
        return _STATIC_CACHE[cache_key]

    prod = read_text(f"{dev_dir}/product_name")
    if prod:
        _STATIC_CACHE[cache_key] = prod
        return prod

    dev_hex = read_text(f"{dev_dir}/device").replace("0x", "").upper()
    rev_hex = read_text(f"{dev_dir}/revision").replace("0x", "").upper()

    amdgpu_ids = Path("/usr/share/libdrm/amdgpu.ids")
    if dev_hex and amdgpu_ids.is_file():
        try:
            with open(amdgpu_ids, "r", encoding="utf-8", errors="replace") as f:
                for line in f:
                    line = line.strip()
                    if not line or line.startswith("#"):
                        continue
                    parts = [p.strip() for p in line.split(",")]
                    if len(parts) >= 3 and parts[0].upper() == dev_hex:
                        if not rev_hex or parts[1].upper() == rev_hex:
                            name = parts[2]
                            _STATIC_CACHE[cache_key] = name
                            return name
        except OSError:
            pass

    # Fallback to lspci
    uevent = read_text(f"{dev_dir}/uevent")
    slot = ""
    for line in uevent.splitlines():
        if line.startswith("PCI_SLOT_NAME="):
            slot = line.split("=", 1)[1].strip()
            break
    if slot:
        try:
            out = subprocess.check_output(["lspci", "-s", slot], text=True, timeout=1.0)
            m = re.search(r"\[([^\]]+)\]", out)
            if m:
                name = m.group(1).split("/")[0].strip()
                _STATIC_CACHE[cache_key] = name
                return name
        except Exception:
            pass

    _STATIC_CACHE[cache_key] = "AMD Radeon GPU"
    return "AMD Radeon GPU"


def get_gpu_stats():
    # 1. Check AMD / Intel DRM sysfs cards
    cards = sorted(glob.glob("/sys/class/drm/card[0-9]*/device"))
    for dev_dir in cards:
        busy_path = f"{dev_dir}/gpu_busy_percent"
        if os.path.exists(busy_path):
            usage = read_int(busy_path, 0) or 0
            vram_used = read_int(f"{dev_dir}/mem_info_vram_used", 0) or 0
            vram_total = read_int(f"{dev_dir}/mem_info_vram_total", 0) or 0
            vram_pct = round((vram_used / vram_total) * 100.0, 1) if vram_total > 0 else 0.0

            temp_edge = None
            temp_hotspot = None
            temp_mem = None
            power_w = None
            power_cap_w = None
            sclk_mhz = None
            mclk_mhz = None
            fan_rpm = None

            for hw in sorted(glob.glob(f"{dev_dir}/hwmon/hwmon*")):
                for ti in sorted(glob.glob(f"{hw}/temp*_input")):
                    prefix = ti[:-6]
                    lbl = read_text(f"{prefix}_label").lower()
                    raw = read_int(ti)
                    if raw is None or raw <= 0:
                        continue
                    deg = round(raw / 1000.0, 1)
                    if lbl in ("edge", ""):
                        if temp_edge is None:
                            temp_edge = deg
                    elif lbl in ("junction", "hotspot"):
                        temp_hotspot = deg
                    elif lbl in ("mem", "memory"):
                        temp_mem = deg

                p_raw = read_int(f"{hw}/power1_average") or read_int(f"{hw}/power1_input")
                if p_raw is not None and p_raw > 0:
                    power_w = round(p_raw / 1_000_000.0, 1)
                p_cap = read_int(f"{hw}/power1_cap")
                if p_cap is not None and p_cap > 0:
                    power_cap_w = round(p_cap / 1_000_000.0, 0)

                for fi in sorted(glob.glob(f"{hw}/freq*_input")):
                    prefix = fi[:-6]
                    lbl = read_text(f"{prefix}_label").lower()
                    f_raw = read_int(fi)
                    if f_raw is not None and f_raw > 0:
                        mhz = round(f_raw / 1_000_000.0)
                        if lbl == "sclk" or sclk_mhz is None:
                            sclk_mhz = mhz
                        elif lbl == "mclk":
                            mclk_mhz = mhz

                fan_raw = read_int(f"{hw}/fan1_input")
                if fan_raw is not None and fan_raw >= 0:
                    fan_rpm = fan_raw

            return {
                "available": True,
                "vendor": "AMD",
                "name": resolve_amd_gpu_name(dev_dir),
                "percent": float(usage),
                "vramUsedBytes": vram_used,
                "vramTotalBytes": vram_total,
                "vramPercent": vram_pct,
                "tempC": temp_edge,
                "hotspotTempC": temp_hotspot,
                "memTempC": temp_mem,
                "powerWatts": power_w,
                "powerCapWatts": power_cap_w,
                "clockMhz": sclk_mhz,
                "memClockMhz": mclk_mhz,
                "fanRpm": fan_rpm,
            }

    # 2. Check NVIDIA via nvidia-smi if present
    if _STATIC_CACHE.get("has_nvidia_smi", True):
        try:
            out = subprocess.check_output(
                [
                    "nvidia-smi",
                    "--query-gpu=name,utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw,power.limit,clocks.current.graphics,clocks.current.memory",
                    "--format=csv,noheader,nounits",
                ],
                text=True,
                stderr=subprocess.DEVNULL,
                timeout=1.5,
            ).strip()
            if out:
                parts = [p.strip() for p in out.splitlines()[0].split(",")]
                if len(parts) >= 5:
                    name = parts[0]
                    usage = float(parts[1]) if parts[1] not in ("[N/A]", "") else 0.0
                    vram_used = int(float(parts[2]) * 1024 * 1024) if parts[2] not in ("[N/A]", "") else 0
                    vram_total = int(float(parts[3]) * 1024 * 1024) if parts[3] not in ("[N/A]", "") else 0
                    vram_pct = round((vram_used / vram_total) * 100.0, 1) if vram_total > 0 else 0.0
                    temp_c = float(parts[4]) if parts[4] not in ("[N/A]", "") else None
                    power_w = float(parts[5]) if len(parts) > 5 and parts[5] not in ("[N/A]", "") else None
                    power_cap_w = float(parts[6]) if len(parts) > 6 and parts[6] not in ("[N/A]", "") else None
                    sclk = int(float(parts[7])) if len(parts) > 7 and parts[7] not in ("[N/A]", "") else None
                    mclk = int(float(parts[8])) if len(parts) > 8 and parts[8] not in ("[N/A]", "") else None
                    return {
                        "available": True,
                        "vendor": "NVIDIA",
                        "name": name,
                        "percent": round(usage, 1),
                        "vramUsedBytes": vram_used,
                        "vramTotalBytes": vram_total,
                        "vramPercent": vram_pct,
                        "tempC": temp_c,
                        "hotspotTempC": None,
                        "memTempC": None,
                        "powerWatts": power_w,
                        "powerCapWatts": power_cap_w,
                        "clockMhz": sclk,
                        "memClockMhz": mclk,
                        "fanRpm": None,
                    }
        except (FileNotFoundError, subprocess.SubprocessError, ValueError):
            _STATIC_CACHE["has_nvidia_smi"] = False

    # 3. Check Intel i915 / xe sysfs frequency fallback
    for card_dir in sorted(glob.glob("/sys/class/drm/card[0-9]*")):
        act_freq = read_int(f"{card_dir}/gt_act_freq_mhz")
        max_freq = read_int(f"{card_dir}/gt_max_freq_mhz")
        if act_freq is not None and max_freq and max_freq > 0:
            pct = round(min(100.0, (act_freq / max_freq) * 100.0), 1)
            return {
                "available": True,
                "vendor": "Intel",
                "name": "Intel Integrated Graphics",
                "percent": pct,
                "vramUsedBytes": 0,
                "vramTotalBytes": 0,
                "vramPercent": 0.0,
                "tempC": None,
                "hotspotTempC": None,
                "memTempC": None,
                "powerWatts": None,
                "powerCapWatts": None,
                "clockMhz": act_freq,
                "memClockMhz": None,
                "fanRpm": None,
            }

    return {
        "available": False,
        "vendor": "",
        "name": "No GPU detected",
        "percent": 0.0,
        "vramUsedBytes": 0,
        "vramTotalBytes": 0,
        "vramPercent": 0.0,
        "tempC": None,
        "hotspotTempC": None,
        "memTempC": None,
        "powerWatts": None,
        "powerCapWatts": None,
        "clockMhz": None,
        "memClockMhz": None,
        "fanRpm": None,
    }


def detect_primary_iface():
    # Check default route in /proc/net/route
    try:
        with open("/proc/net/route", "r", encoding="utf-8") as f:
            next(f, None)
            for line in f:
                parts = line.strip().split()
                if len(parts) >= 2 and parts[1] == "00000000":
                    return parts[0]
    except OSError:
        pass

    # Fallback: first UP non-virtual interface
    for net_dir in sorted(glob.glob("/sys/class/net/*")):
        iface = os.path.basename(net_dir)
        if iface == "lo" or iface.startswith(("docker", "veth", "br-", "virbr")):
            continue
        if read_text(f"{net_dir}/operstate") == "up":
            return iface
    return ""


def read_net_bytes():
    stats = {}
    try:
        with open("/proc/net/dev", "r", encoding="utf-8") as f:
            for line in f:
                if ":" not in line:
                    continue
                iface, rest = line.split(":", 1)
                iface = iface.strip()
                fields = rest.split()
                if len(fields) >= 9:
                    stats[iface] = (int(fields[0]), int(fields[8]))
    except OSError:
        pass
    return stats


def get_iface_ip(iface):
    if not iface:
        return ""
    now = time.monotonic()
    cached = _STATIC_CACHE.get("ip_cache")
    if cached and cached[0] == iface and (now - cached[1]) < 15.0:
        return cached[2]
    ip_addr = ""
    try:
        out = subprocess.check_output(
            ["ip", "-4", "-o", "addr", "show", "dev", iface],
            text=True,
            stderr=subprocess.DEVNULL,
            timeout=0.8,
        )
        m = re.search(r"inet\s+([0-9.]+)", out)
        if m:
            ip_addr = m.group(1)
    except Exception:
        pass
    _STATIC_CACHE["ip_cache"] = (iface, now, ip_addr)
    return ip_addr


def get_top_processes():
    now = time.monotonic()
    cached = _STATIC_CACHE.get("top_procs")
    if cached and (now - cached[0]) < 4.0:
        return cached[1]

    procs = []
    try:
        out = subprocess.check_output(
            ["ps", "-eo", "comm,%cpu,%mem", "--sort=-%cpu", "--no-headers"],
            text=True,
            stderr=subprocess.DEVNULL,
            timeout=0.8,
        )
        for line in out.splitlines()[:4]:
            parts = line.strip().split(None, 2)
            if len(parts) == 3:
                name = parts[0]
                if name in ("ps", "collect.py"):
                    continue
                try:
                    procs.append({
                        "name": name,
                        "cpu": round(float(parts[1]), 1),
                        "mem": round(float(parts[2]), 1),
                    })
                except ValueError:
                    pass
                if len(procs) >= 3:
                    break
    except Exception:
        pass

    _STATIC_CACHE["top_procs"] = (now, procs)
    return procs


def load_prev_state():
    try:
        with open(STATE_FILE, "r", encoding="utf-8") as f:
            return json.load(f)
    except Exception:
        return None


def save_state(state):
    try:
        STATE_DIR.mkdir(parents=True, exist_ok=True)
        tmp = STATE_FILE.with_suffix(".tmp")
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(state, f)
        tmp.replace(STATE_FILE)
    except OSError:
        pass


def collect_snapshot(mem_state=None):
    now = time.time()
    prev = mem_state if mem_state is not None else load_prev_state()

    cur_cpus = read_proc_stat()
    cur_net = read_net_bytes()

    if not prev or (now - prev.get("timestamp", 0)) > 30.0 or (now - prev.get("timestamp", 0)) <= 0.02:
        time.sleep(0.12)
        prev_cpus = cur_cpus
        prev_net = cur_net
        prev_ts = now
        now = time.time()
        cur_cpus = read_proc_stat()
        cur_net = read_net_bytes()
    else:
        prev_cpus = {k: tuple(v) for k, v in prev.get("cpus", {}).items()}
        prev_net = {k: tuple(v) for k, v in prev.get("net", {}).items()}
        prev_ts = prev.get("timestamp", now - 1.0)

    dt = max(0.05, now - prev_ts)

    cpu_pct, per_core = compute_cpu_usage(prev_cpus, cur_cpus)
    cpu_static = get_cpu_static()
    avg_freq, max_freq = get_cpu_freq_mhz()
    cpu_temp = get_cpu_temp()

    load_parts = read_text("/proc/loadavg", "0.00 0.00 0.00").split()
    load_1m = float(load_parts[0]) if len(load_parts) > 0 else 0.0
    load_5m = float(load_parts[1]) if len(load_parts) > 1 else 0.0
    load_15m = float(load_parts[2]) if len(load_parts) > 2 else 0.0

    uptime_raw = read_text("/proc/uptime", "0").split()[0]
    uptime_sec = int(float(uptime_raw)) if uptime_raw else 0

    iface = detect_primary_iface()
    rx_rate = 0.0
    tx_rate = 0.0
    rx_total = 0
    tx_total = 0
    if iface and iface in cur_net:
        rx_total, tx_total = cur_net[iface]
        if iface in prev_net:
            p_rx, p_tx = prev_net[iface]
            rx_rate = max(0.0, (rx_total - p_rx) / dt)
            tx_rate = max(0.0, (tx_total - p_tx) / dt)

    is_wifi = os.path.isdir(f"/sys/class/net/{iface}/wireless") if iface else False
    link_speed = read_int(f"/sys/class/net/{iface}/speed") if iface else None
    operstate = read_text(f"/sys/class/net/{iface}/operstate", "down") if iface else "down"

    new_state = {
        "timestamp": now,
        "cpus": cur_cpus,
        "net": cur_net,
    }
    save_state(new_state)

    snapshot = {
        "ok": True,
        "timestamp": int(now),
        "hostname": socket.gethostname(),
        "uptimeSec": uptime_sec,
        "cpu": {
            "model": cpu_static["model"],
            "cores": cpu_static["cores"],
            "threads": cpu_static["threads"],
            "percent": cpu_pct,
            "perCore": per_core,
            "tempC": cpu_temp,
            "freqMhz": avg_freq,
            "maxFreqMhz": max_freq,
            "load1m": load_1m,
            "load5m": load_5m,
            "load15m": load_15m,
        },
        "ram": get_memory_stats(),
        "gpu": get_gpu_stats(),
        "network": {
            "interface": iface or "offline",
            "connected": bool(iface and operstate == "up"),
            "kind": "wifi" if is_wifi else ("ethernet" if iface else "disconnected"),
            "ip": get_iface_ip(iface),
            "linkSpeedMbps": link_speed if (link_speed and link_speed > 0) else None,
            "rxBytesPerSec": round(rx_rate, 1),
            "txBytesPerSec": round(tx_rate, 1),
            "rxTotalBytes": rx_total,
            "txTotalBytes": tx_total,
        },
        "topProcesses": get_top_processes(),
    }
    return snapshot, new_state


def main():
    signal.signal(signal.SIGINT, lambda *_: sys.exit(0))
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))

    if len(sys.argv) >= 2 and sys.argv[1] == "--watch":
        interval = 2.0
        if len(sys.argv) >= 3:
            try:
                interval = max(0.5, float(sys.argv[2]))
            except ValueError:
                interval = 2.0

        mem_state = load_prev_state()
        while True:
            if os.getppid() == 1:
                break
            snap, mem_state = collect_snapshot(mem_state)
            sys.stdout.write(json.dumps(snap, separators=(",", ":")) + "\n")
            sys.stdout.flush()
            time.sleep(interval)
    else:
        snap, _ = collect_snapshot()
        sys.stdout.write(json.dumps(snap, separators=(",", ":")) + "\n")
        sys.stdout.flush()


if __name__ == "__main__":
    main()

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct StaticCache {
    hostname: String,
    cpu_model: String,
    cpu_cores: usize,
    cpu_threads: usize,
    gpu_names: HashMap<String, String>,
    has_nvidia_smi: bool,
    cached_ip: Option<(String, Instant, String)>,
}

impl StaticCache {
    fn new() -> Self {
        let hostname = read_trimmed("/etc/hostname")
            .or_else(|| read_trimmed("/proc/sys/kernel/hostname"))
            .unwrap_or_else(|| "System".to_string());

        let (cpu_model, cpu_cores, cpu_threads) = detect_cpu_static();
        let has_nvidia_smi = Path::new("/usr/bin/nvidia-smi").exists();

        Self {
            hostname,
            cpu_model,
            cpu_cores,
            cpu_threads,
            gpu_names: HashMap::new(),
            has_nvidia_smi,
            cached_ip: None,
        }
    }
}

struct PrevState {
    instant: Instant,
    cpus: HashMap<String, (u64, u64)>,
    net: HashMap<String, (u64, u64)>,
    procs: HashMap<u32, (String, u64, u64)>, // pid -> (comm, utime+stime, rss_bytes)
    total_jiffies: u64,
}

fn read_trimmed<P: AsRef<Path>>(path: P) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn read_u64<P: AsRef<Path>>(path: P) -> Option<u64> {
    let s = read_trimmed(path)?;
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<u64>().ok()
    }
}

fn read_i64<P: AsRef<Path>>(path: P) -> Option<i64> {
    let s = read_trimmed(path)?;
    s.parse::<i64>().ok()
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn opt_f64_json(v: Option<f64>, decimals: usize) -> String {
    match v {
        Some(val) if val.is_finite() => format!("{:.*}", decimals, val),
        _ => "null".to_string(),
    }
}

fn opt_u64_json(v: Option<u64>) -> String {
    match v {
        Some(val) => val.to_string(),
        None => "null".to_string(),
    }
}

fn clean_cpu_model(raw: &str) -> String {
    let mut s = raw
        .replace("(R)", "")
        .replace("(TM)", "")
        .replace("(tm)", "");
    if let Some(idx) = s.find(" @") {
        s.truncate(idx);
    }
    let words: Vec<&str> = s
        .split_whitespace()
        .filter(|w| {
            let l = w.to_ascii_lowercase();
            l != "processor"
                && !l.ends_with("-core")
                && l != "eight-core"
                && l != "six-core"
                && l != "quad-core"
                && l != "twelve-core"
                && l != "sixteen-core"
        })
        .collect();
    let joined = words.join(" ");
    if joined.is_empty() {
        raw.trim().to_string()
    } else {
        joined
    }
}

fn detect_cpu_static() -> (String, usize, usize) {
    let mut model = String::from("Processor");
    let mut phys_cores = HashSet::new();
    let mut threads = 0usize;

    if let Ok(content) = fs::read_to_string("/proc/cpuinfo") {
        let mut phys_id = String::from("0");
        for line in content.lines() {
            if let Some((k, v)) = line.split_once(':') {
                let k = k.trim();
                let v = v.trim();
                match k {
                    "processor" => threads += 1,
                    "model name" if model == "Processor" => model = clean_cpu_model(v),
                    "physical id" => phys_id = v.to_string(),
                    "core id" => {
                        phys_cores.insert((phys_id.clone(), v.to_string()));
                    }
                    _ => {}
                }
            }
        }
    }

    let threads = threads.max(1);
    let cores = if phys_cores.is_empty() { threads } else { phys_cores.len() };
    (model, cores, threads)
}

fn read_proc_stat() -> (HashMap<String, (u64, u64)>, u64) {
    let mut cpus = HashMap::new();
    let mut total_jiffies = 0u64;
    if let Ok(content) = fs::read_to_string("/proc/stat") {
        for line in content.lines() {
            if !line.starts_with("cpu") {
                break;
            }
            let mut parts = line.split_whitespace();
            if let Some(name) = parts.next() {
                let nums: Vec<u64> = parts.filter_map(|p| p.parse::<u64>().ok()).collect();
                if nums.len() >= 4 {
                    let idle = nums[3] + nums.get(4).copied().unwrap_or(0);
                    let total: u64 = nums.iter().sum();
                    if name == "cpu" {
                        total_jiffies = total;
                    }
                    cpus.insert(name.to_string(), (idle, total));
                }
            }
        }
    }
    (cpus, total_jiffies)
}

fn compute_cpu_usage(
    prev: &HashMap<String, (u64, u64)>,
    cur: &HashMap<String, (u64, u64)>,
) -> (f64, Vec<f64>) {
    let mut overall = 0.0;
    if let (Some(&(p_idle, p_tot)), Some(&(c_idle, c_tot))) = (prev.get("cpu"), cur.get("cpu")) {
        let d_tot = c_tot.saturating_sub(p_tot);
        let d_idle = c_idle.saturating_sub(p_idle);
        if d_tot > 0 {
            overall = ((d_tot.saturating_sub(d_idle) as f64) / (d_tot as f64) * 100.0).clamp(0.0, 100.0);
        }
    }

    let mut per_core = Vec::new();
    let mut idx = 0usize;
    loop {
        let key = format!("cpu{}", idx);
        let Some(&(c_idle, c_tot)) = cur.get(&key) else { break };
        let pct = if let Some(&(p_idle, p_tot)) = prev.get(&key) {
            let d_tot = c_tot.saturating_sub(p_tot);
            let d_idle = c_idle.saturating_sub(p_idle);
            if d_tot > 0 {
                ((d_tot.saturating_sub(d_idle) as f64) / (d_tot as f64) * 100.0).clamp(0.0, 100.0)
            } else {
                0.0
            }
        } else {
            0.0
        };
        per_core.push((pct * 10.0).round() / 10.0);
        idx += 1;
    }

    ((overall * 10.0).round() / 10.0, per_core)
}

fn read_cpu_freq_mhz() -> (u64, u64) {
    let mut freqs = Vec::new();
    if let Ok(entries) = fs::read_dir("/sys/devices/system/cpu") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("cpu") && name_str[3..].chars().all(|c| c.is_ascii_digit()) && name_str.len() > 3 {
                let p = entry.path().join("cpufreq/scaling_cur_freq");
                if let Some(khz) = read_u64(p) {
                    if khz > 0 {
                        freqs.push(khz / 1000);
                    }
                }
            }
        }
    }
    if !freqs.is_empty() {
        let avg = freqs.iter().sum::<u64>() / (freqs.len() as u64);
        let max = freqs.into_iter().max().unwrap_or(0);
        return (avg, max);
    }

    if let Ok(content) = fs::read_to_string("/proc/cpuinfo") {
        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("cpu MHz") {
                if let Some((_, val)) = rest.split_once(':') {
                    if let Ok(mhz) = val.trim().parse::<f64>() {
                        freqs.push(mhz.round() as u64);
                    }
                }
            }
        }
    }
    if !freqs.is_empty() {
        let avg = freqs.iter().sum::<u64>() / (freqs.len() as u64);
        let max = freqs.into_iter().max().unwrap_or(0);
        return (avg, max);
    }
    (0, 0)
}

fn read_cpu_temp() -> Option<f64> {
    let mut fallback: Option<f64> = None;
    let Ok(entries) = fs::read_dir("/sys/class/hwmon") else { return None };
    let mut hwmons: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    hwmons.sort();

    for hw in hwmons {
        let Some(name) = read_trimmed(hw.join("name")) else { continue };
        if !matches!(name.as_str(), "k10temp" | "zenpower" | "coretemp" | "cpu_thermal" | "acpitz") {
            continue;
        }
        let mut label_map: HashMap<String, f64> = HashMap::new();
        for i in 1..=12 {
            let input_path = hw.join(format!("temp{}_input", i));
            let Some(raw) = read_i64(&input_path) else { continue };
            if raw <= 0 {
                continue;
            }
            let deg = (raw as f64) / 1000.0;
            if fallback.is_none() {
                fallback = Some(deg);
            }
            let lbl = read_trimmed(hw.join(format!("temp{}_label", i)))
                .unwrap_or_default()
                .to_ascii_lowercase();
            label_map.insert(lbl, deg);
        }
        for pref in ["tdie", "tccd1", "package id 0", "core 0", "tctl"] {
            if let Some(&deg) = label_map.get(pref) {
                return Some((deg * 10.0).round() / 10.0);
            }
        }
    }
    fallback.map(|d| (d * 10.0).round() / 10.0)
}

struct MemStats {
    used: u64,
    avail: u64,
    total: u64,
    cached: u64,
    pct: f64,
    swap_used: u64,
    swap_total: u64,
    swap_pct: f64,
}

fn read_memory_stats() -> MemStats {
    let mut map: HashMap<String, u64> = HashMap::new();
    if let Ok(content) = fs::read_to_string("/proc/meminfo") {
        for line in content.lines() {
            if let Some((k, rest)) = line.split_once(':') {
                if let Some(num_str) = rest.split_whitespace().next() {
                    if let Ok(kb) = num_str.parse::<u64>() {
                        map.insert(k.trim().to_string(), kb * 1024);
                    }
                }
            }
        }
    }
    let total = map.get("MemTotal").copied().unwrap_or(0);
    let avail = map
        .get("MemAvailable")
        .or_else(|| map.get("MemFree"))
        .copied()
        .unwrap_or(0);
    let used = total.saturating_sub(avail);
    let cached = map.get("Cached").copied().unwrap_or(0)
        + map.get("SReclaimable").copied().unwrap_or(0)
        + map.get("Buffers").copied().unwrap_or(0);
    let pct = if total > 0 {
        ((used as f64) / (total as f64) * 1000.0).round() / 10.0
    } else {
        0.0
    };

    let swap_total = map.get("SwapTotal").copied().unwrap_or(0);
    let swap_free = map.get("SwapFree").copied().unwrap_or(0);
    let swap_used = swap_total.saturating_sub(swap_free);
    let swap_pct = if swap_total > 0 {
        ((swap_used as f64) / (swap_total as f64) * 1000.0).round() / 10.0
    } else {
        0.0
    };

    MemStats {
        used,
        avail,
        total,
        cached,
        pct,
        swap_used,
        swap_total,
        swap_pct,
    }
}

fn resolve_amd_gpu_name(dev_dir: &Path, cache: &mut StaticCache) -> String {
    let key = dev_dir.to_string_lossy().to_string();
    if let Some(existing) = cache.gpu_names.get(&key) {
        return existing.clone();
    }

    if let Some(prod) = read_trimmed(dev_dir.join("product_name")) {
        cache.gpu_names.insert(key, prod.clone());
        return prod;
    }

    let dev_hex = read_trimmed(dev_dir.join("device"))
        .unwrap_or_default()
        .trim_start_matches("0x")
        .to_ascii_uppercase();
    let rev_hex = read_trimmed(dev_dir.join("revision"))
        .unwrap_or_default()
        .trim_start_matches("0x")
        .to_ascii_uppercase();

    if !dev_hex.is_empty() {
        if let Ok(ids) = fs::read_to_string("/usr/share/libdrm/amdgpu.ids") {
            for line in ids.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let parts: Vec<&str> = line.splitn(3, ',').map(|s| s.trim()).collect();
                if parts.len() == 3 && parts[0].eq_ignore_ascii_case(&dev_hex) {
                    if rev_hex.is_empty() || parts[1].eq_ignore_ascii_case(&rev_hex) {
                        let name = parts[2].to_string();
                        cache.gpu_names.insert(key, name.clone());
                        return name;
                    }
                }
            }
        }
    }

    let fallback = String::from("AMD Radeon GPU");
    cache.gpu_names.insert(key, fallback.clone());
    fallback
}

struct GpuStats {
    available: bool,
    vendor: String,
    name: String,
    percent: f64,
    vram_used: u64,
    vram_total: u64,
    vram_pct: f64,
    temp_c: Option<f64>,
    hotspot_temp_c: Option<f64>,
    mem_temp_c: Option<f64>,
    power_w: Option<f64>,
    power_cap_w: Option<f64>,
    clock_mhz: Option<u64>,
    mem_clock_mhz: Option<u64>,
    fan_rpm: Option<u64>,
}

fn read_gpu_stats(cache: &mut StaticCache) -> GpuStats {
    if let Ok(entries) = fs::read_dir("/sys/class/drm") {
        let mut cards: Vec<PathBuf> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with("card") && name[4..].chars().all(|c| c.is_ascii_digit()) && name.len() > 4 {
                    Some(e.path())
                } else {
                    None
                }
            })
            .collect();
        cards.sort();

        for card in &cards {
            let dev_dir = card.join("device");
            let busy_path = dev_dir.join("gpu_busy_percent");
            if busy_path.exists() {
                let usage = read_u64(&busy_path).unwrap_or(0) as f64;
                let vram_used = read_u64(dev_dir.join("mem_info_vram_used")).unwrap_or(0);
                let vram_total = read_u64(dev_dir.join("mem_info_vram_total")).unwrap_or(0);
                let vram_pct = if vram_total > 0 {
                    ((vram_used as f64) / (vram_total as f64) * 1000.0).round() / 10.0
                } else {
                    0.0
                };

                let mut temp_edge = None;
                let mut temp_hotspot = None;
                let mut temp_mem = None;
                let mut power_w = None;
                let mut power_cap_w = None;
                let mut sclk_mhz = None;
                let mut mclk_mhz = None;
                let mut fan_rpm = None;

                if let Ok(hwmons) = fs::read_dir(dev_dir.join("hwmon")) {
                    for hw_entry in hwmons.flatten() {
                        let hw = hw_entry.path();
                        for i in 1..=6 {
                            let ti = hw.join(format!("temp{}_input", i));
                            if let Some(raw) = read_i64(&ti) {
                                if raw > 0 {
                                    let deg = ((raw as f64) / 100.0).round() / 10.0;
                                    let lbl = read_trimmed(hw.join(format!("temp{}_label", i)))
                                        .unwrap_or_default()
                                        .to_ascii_lowercase();
                                    match lbl.as_str() {
                                        "edge" | "" => {
                                            if temp_edge.is_none() {
                                                temp_edge = Some(deg);
                                            }
                                        }
                                        "junction" | "hotspot" => temp_hotspot = Some(deg),
                                        "mem" | "memory" => temp_mem = Some(deg),
                                        _ => {}
                                    }
                                }
                            }
                        }

                        if let Some(p_raw) = read_u64(hw.join("power1_average")).or_else(|| read_u64(hw.join("power1_input"))) {
                            if p_raw > 0 {
                                power_w = Some(((p_raw as f64) / 100_000.0).round() / 10.0);
                            }
                        }
                        if let Some(p_cap) = read_u64(hw.join("power1_cap")) {
                            if p_cap > 0 {
                                power_cap_w = Some(((p_cap as f64) / 1_000_000.0).round());
                            }
                        }

                        for i in 1..=4 {
                            let fi = hw.join(format!("freq{}_input", i));
                            if let Some(f_raw) = read_u64(&fi) {
                                if f_raw > 0 {
                                    let mhz = f_raw / 1_000_000;
                                    let lbl = read_trimmed(hw.join(format!("freq{}_label", i)))
                                        .unwrap_or_default()
                                        .to_ascii_lowercase();
                                    if lbl == "sclk" || sclk_mhz.is_none() {
                                        sclk_mhz = Some(mhz);
                                    } else if lbl == "mclk" {
                                        mclk_mhz = Some(mhz);
                                    }
                                }
                            }
                        }

                        if let Some(f_rpm) = read_u64(hw.join("fan1_input")) {
                            fan_rpm = Some(f_rpm);
                        }
                    }
                }

                return GpuStats {
                    available: true,
                    vendor: "AMD".to_string(),
                    name: resolve_amd_gpu_name(&dev_dir, cache),
                    percent: usage,
                    vram_used,
                    vram_total,
                    vram_pct,
                    temp_c: temp_edge,
                    hotspot_temp_c: temp_hotspot,
                    mem_temp_c: temp_mem,
                    power_w,
                    power_cap_w,
                    clock_mhz: sclk_mhz,
                    mem_clock_mhz: mclk_mhz,
                    fan_rpm,
                };
            }
        }

        // Intel i915 / xe fallback
        for card in &cards {
            if let (Some(act), Some(max)) = (
                read_u64(card.join("gt_act_freq_mhz")),
                read_u64(card.join("gt_max_freq_mhz")),
            ) {
                if max > 0 {
                    let pct = (((act as f64) / (max as f64) * 1000.0).round() / 10.0).clamp(0.0, 100.0);
                    return GpuStats {
                        available: true,
                        vendor: "Intel".to_string(),
                        name: "Intel Integrated Graphics".to_string(),
                        percent: pct,
                        vram_used: 0,
                        vram_total: 0,
                        vram_pct: 0.0,
                        temp_c: None,
                        hotspot_temp_c: None,
                        mem_temp_c: None,
                        power_w: None,
                        power_cap_w: None,
                        clock_mhz: Some(act),
                        mem_clock_mhz: None,
                        fan_rpm: None,
                    };
                }
            }
        }
    }

    // NVIDIA fallback via nvidia-smi
    if cache.has_nvidia_smi {
        if let Ok(output) = Command::new("nvidia-smi")
            .args([
                "--query-gpu=name,utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw,power.limit,clocks.current.graphics,clocks.current.memory",
                "--format=csv,noheader,nounits",
            ])
            .output()
        {
            if output.status.success() {
                let s = String::from_utf8_lossy(&output.stdout);
                if let Some(first_line) = s.lines().next() {
                    let parts: Vec<&str> = first_line.split(',').map(|p| p.trim()).collect();
                    if parts.len() >= 5 {
                        let name = parts[0].to_string();
                        let usage = parts[1].parse::<f64>().unwrap_or(0.0);
                        let vram_used = (parts[2].parse::<f64>().unwrap_or(0.0) * 1024.0 * 1024.0) as u64;
                        let vram_total = (parts[3].parse::<f64>().unwrap_or(0.0) * 1024.0 * 1024.0) as u64;
                        let vram_pct = if vram_total > 0 {
                            ((vram_used as f64) / (vram_total as f64) * 1000.0).round() / 10.0
                        } else {
                            0.0
                        };
                        let temp_c = parts[4].parse::<f64>().ok();
                        let power_w = parts.get(5).and_then(|v| v.parse::<f64>().ok());
                        let power_cap_w = parts.get(6).and_then(|v| v.parse::<f64>().ok());
                        let sclk = parts.get(7).and_then(|v| v.parse::<u64>().ok());
                        let mclk = parts.get(8).and_then(|v| v.parse::<u64>().ok());
                        return GpuStats {
                            available: true,
                            vendor: "NVIDIA".to_string(),
                            name,
                            percent: usage,
                            vram_used,
                            vram_total,
                            vram_pct,
                            temp_c,
                            hotspot_temp_c: None,
                            mem_temp_c: None,
                            power_w,
                            power_cap_w,
                            clock_mhz: sclk,
                            mem_clock_mhz: mclk,
                            fan_rpm: None,
                        };
                    }
                }
            } else {
                cache.has_nvidia_smi = false;
            }
        } else {
            cache.has_nvidia_smi = false;
        }
    }

    GpuStats {
        available: false,
        vendor: String::new(),
        name: "No GPU detected".to_string(),
        percent: 0.0,
        vram_used: 0,
        vram_total: 0,
        vram_pct: 0.0,
        temp_c: None,
        hotspot_temp_c: None,
        mem_temp_c: None,
        power_w: None,
        power_cap_w: None,
        clock_mhz: None,
        mem_clock_mhz: None,
        fan_rpm: None,
    }
}

fn detect_primary_iface() -> String {
    if let Ok(content) = fs::read_to_string("/proc/net/route") {
        for line in content.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 && parts[1] == "00000000" {
                return parts[0].to_string();
            }
        }
    }
    if let Ok(entries) = fs::read_dir("/sys/class/net") {
        let mut names: Vec<String> = entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        for iface in names {
            if iface == "lo"
                || iface.starts_with("docker")
                || iface.starts_with("veth")
                || iface.starts_with("br-")
                || iface.starts_with("virbr")
            {
                continue;
            }
            if read_trimmed(format!("/sys/class/net/{}/operstate", iface)).as_deref() == Some("up") {
                return iface;
            }
        }
    }
    String::new()
}

fn read_net_bytes() -> HashMap<String, (u64, u64)> {
    let mut map = HashMap::new();
    if let Ok(content) = fs::read_to_string("/proc/net/dev") {
        for line in content.lines() {
            if let Some((iface, rest)) = line.split_once(':') {
                let fields: Vec<&str> = rest.split_whitespace().collect();
                if fields.len() >= 9 {
                    let rx = fields[0].parse::<u64>().unwrap_or(0);
                    let tx = fields[8].parse::<u64>().unwrap_or(0);
                    map.insert(iface.trim().to_string(), (rx, tx));
                }
            }
        }
    }
    map
}

fn get_iface_ip(iface: &str, cache: &mut StaticCache) -> String {
    if iface.is_empty() {
        return String::new();
    }
    let now = Instant::now();
    if let Some((ref c_iface, ts, ref ip)) = cache.cached_ip {
        if c_iface == iface && now.duration_since(ts) < Duration::from_secs(15) {
            return ip.clone();
        }
    }

    let mut found_ip = String::new();
    if let Ok(out) = Command::new("ip")
        .args(["-4", "-o", "addr", "show", "dev", iface])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for token_pair in text.split_whitespace().collect::<Vec<_>>().windows(2) {
            if token_pair[0] == "inet" {
                if let Some((ip, _)) = token_pair[1].split_once('/') {
                    found_ip = ip.to_string();
                    break;
                }
            }
        }
    }

    cache.cached_ip = Some((iface.to_string(), now, found_ip.clone()));
    found_ip
}

fn read_proc_snapshot() -> HashMap<u32, (String, u64, u64)> {
    let mut out = HashMap::new();
    let Ok(entries) = fs::read_dir("/proc") else { return out };
    let page_size = 4096u64;
    let self_pid = std::process::id();

    for entry in entries.flatten() {
        let fname = entry.file_name();
        let Some(s) = fname.to_str() else { continue };
        if !s.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Ok(pid) = s.parse::<u32>() else { continue };
        if pid == self_pid {
            continue;
        }
        let stat_path = entry.path().join("stat");
        let Ok(stat_str) = fs::read_to_string(stat_path) else { continue };
        let Some(open_paren) = stat_str.find('(') else { continue };
        let Some(close_paren) = stat_str.rfind(')') else { continue };
        if close_paren <= open_paren || close_paren + 2 >= stat_str.len() {
            continue;
        }
        let comm = stat_str[open_paren + 1..close_paren].to_string();
        let after = &stat_str[close_paren + 2..];
        let fields: Vec<&str> = after.split_whitespace().collect();
        // field 0 is state (3rd col), utime is index 11 (14th col), stime is index 12 (15th col), rss is index 21 (24th col)
        if fields.len() > 21 {
            let utime = fields[11].parse::<u64>().unwrap_or(0);
            let stime = fields[12].parse::<u64>().unwrap_or(0);
            let rss_pages = fields[21].parse::<u64>().unwrap_or(0);
            out.insert(pid, (comm, utime + stime, rss_pages * page_size));
        }
    }
    out
}

fn compute_top_procs(
    prev: &HashMap<u32, (String, u64, u64)>,
    cur: &HashMap<u32, (String, u64, u64)>,
    d_total_jiffies: u64,
    num_threads: usize,
    mem_total: u64,
) -> Vec<(String, f64, f64)> {
    if d_total_jiffies == 0 {
        return Vec::new();
    }
    let mut agg: HashMap<String, (u64, u64)> = HashMap::new();
    for (pid, (comm, c_ticks, rss)) in cur {
        if let Some((_, p_ticks, _)) = prev.get(pid) {
            let d_ticks = c_ticks.saturating_sub(*p_ticks);
            if d_ticks > 0 || *rss > 0 {
                let entry = agg.entry(comm.clone()).or_insert((0, 0));
                entry.0 += d_ticks;
                entry.1 += *rss;
            }
        }
    }

    let scale = (num_threads.max(1) as f64) * 100.0 / (d_total_jiffies as f64);
    let mut list: Vec<(String, f64, f64)> = agg
        .into_iter()
        .filter_map(|(name, (d_ticks, rss))| {
            let cpu_pct = ((d_ticks as f64) * scale * 10.0).round() / 10.0;
            if cpu_pct <= 0.1 {
                return None;
            }
            let mem_pct = if mem_total > 0 {
                ((rss as f64) / (mem_total as f64) * 1000.0).round() / 10.0
            } else {
                0.0
            };
            Some((name, cpu_pct, mem_pct))
        })
        .collect();

    list.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    list.truncate(3);
    list
}

fn state_file_path() -> PathBuf {
    let base = env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(base).join("pc-stats").join("state.dat")
}

fn save_disk_state(unix_ts: f64, cpus: &HashMap<String, (u64, u64)>, net: &HashMap<String, (u64, u64)>) {
    let path = state_file_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut out = format!("ts {:.4}\n", unix_ts);
    for (k, (idle, tot)) in cpus {
        out.push_str(&format!("c {} {} {}\n", k, idle, tot));
    }
    for (k, (rx, tx)) in net {
        out.push_str(&format!("n {} {} {}\n", k, rx, tx));
    }
    let tmp = path.with_extension("tmp");
    if fs::write(&tmp, out).is_ok() {
        let _ = fs::rename(&tmp, &path);
    }
}

fn load_disk_state() -> Option<(f64, HashMap<String, (u64, u64)>, HashMap<String, (u64, u64)>)> {
    let content = fs::read_to_string(state_file_path()).ok()?;
    let mut ts = 0.0;
    let mut cpus = HashMap::new();
    let mut net = HashMap::new();
    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        match parts.as_slice() {
            ["ts", v] => ts = v.parse::<f64>().ok()?,
            ["c", name, idle, tot] => {
                cpus.insert(
                    name.to_string(),
                    (idle.parse().ok()?, tot.parse().ok()?),
                );
            }
            ["n", name, rx, tx] => {
                net.insert(name.to_string(), (rx.parse().ok()?, tx.parse().ok()?));
            }
            _ => {}
        }
    }
    if ts > 0.0 {
        Some((ts, cpus, net))
    } else {
        None
    }
}

fn parent_is_init() -> bool {
    if let Ok(stat) = fs::read_to_string("/proc/self/stat") {
        if let Some(close) = stat.rfind(')') {
            let after = &stat[close + 2..];
            let mut parts = after.split_whitespace();
            let _state = parts.next();
            if let Some(ppid_str) = parts.next() {
                if ppid_str == "1" {
                    return true;
                }
            }
        }
    }
    false
}

fn collect_and_emit(cache: &mut StaticCache, prev: Option<PrevState>) -> PrevState {
    let now_inst = Instant::now();
    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    let (mut cur_cpus, mut cur_jiffies) = read_proc_stat();
    let mut cur_net = read_net_bytes();
    let mut cur_procs = read_proc_snapshot();

    let (prev_cpus, prev_net, prev_procs, prev_jiffies, dt) = if let Some(p) = prev {
        let elapsed = now_inst.duration_since(p.instant).as_secs_f64().max(0.05);
        (p.cpus, p.net, p.procs, p.total_jiffies, elapsed)
    } else if let Some((disk_ts, d_cpus, d_net)) = load_disk_state().filter(|(ts, _, _)| {
        let diff = now_unix - *ts;
        diff > 0.04 && diff < 25.0
    }) {
        (d_cpus, d_net, HashMap::new(), cur_jiffies, (now_unix - disk_ts).max(0.05))
    } else {
        thread::sleep(Duration::from_millis(110));
        let p_cpus = cur_cpus;
        let p_net = cur_net;
        let p_procs = cur_procs;
        let p_jiffies = cur_jiffies;
        (cur_cpus, cur_jiffies) = read_proc_stat();
        cur_net = read_net_bytes();
        cur_procs = read_proc_snapshot();
        (p_cpus, p_net, p_procs, p_jiffies, 0.11)
    };

    save_disk_state(now_unix, &cur_cpus, &cur_net);

    let (cpu_pct, per_core) = compute_cpu_usage(&prev_cpus, &cur_cpus);
    let (avg_freq, max_freq) = read_cpu_freq_mhz();
    let cpu_temp = read_cpu_temp();

    let load_str = read_trimmed("/proc/loadavg").unwrap_or_default();
    let mut load_parts = load_str.split_whitespace();
    let load1 = load_parts.next().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
    let load5 = load_parts.next().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
    let load15 = load_parts.next().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);

    let uptime_sec = read_trimmed("/proc/uptime")
        .and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok())
        .unwrap_or(0.0) as u64;

    let mem = read_memory_stats();
    let gpu = read_gpu_stats(cache);

    let iface = detect_primary_iface();
    let (rx_total, tx_total) = if !iface.is_empty() {
        cur_net.get(&iface).copied().unwrap_or((0, 0))
    } else {
        (0, 0)
    };
    let (rx_rate, tx_rate) = if !iface.is_empty() {
        if let Some(&(p_rx, p_tx)) = prev_net.get(&iface) {
            (
                (rx_total.saturating_sub(p_rx) as f64) / dt,
                (tx_total.saturating_sub(p_tx) as f64) / dt,
            )
        } else {
            (0.0, 0.0)
        }
    } else {
        (0.0, 0.0)
    };

    let is_wifi = !iface.is_empty() && Path::new(&format!("/sys/class/net/{}/wireless", iface)).is_dir();
    let link_speed = if !iface.is_empty() {
        read_i64(format!("/sys/class/net/{}/speed", iface))
            .filter(|&v| v > 0)
            .map(|v| v as u64)
    } else {
        None
    };
    let connected = !iface.is_empty()
        && read_trimmed(format!("/sys/class/net/{}/operstate", iface)).as_deref() == Some("up");
    let ip_addr = if connected {
        get_iface_ip(&iface, cache)
    } else {
        String::new()
    };

    let d_jiffies = cur_jiffies.saturating_sub(prev_jiffies);
    let top_procs = compute_top_procs(&prev_procs, &cur_procs, d_jiffies, cache.cpu_threads, mem.total);

    let per_core_json: Vec<String> = per_core.iter().map(|v| format!("{:.1}", v)).collect();
    let top_procs_json: Vec<String> = top_procs
        .iter()
        .map(|(n, c, m)| {
            format!(
                "{{\"name\":{},\"cpu\":{:.1},\"mem\":{:.1}}}",
                json_escape(n),
                c,
                m
            )
        })
        .collect();

    let net_kind = if !connected {
        "disconnected"
    } else if is_wifi {
        "wifi"
    } else {
        "ethernet"
    };

    let json = format!(
        "{{\"ok\":true,\"timestamp\":{},\"hostname\":{},\"uptimeSec\":{},\"cpu\":{{\"model\":{},\"cores\":{},\"threads\":{},\"percent\":{:.1},\"perCore\":[{}],\"tempC\":{},\"freqMhz\":{},\"maxFreqMhz\":{},\"load1m\":{:.2},\"load5m\":{:.2},\"load15m\":{:.2}}},\"ram\":{{\"usedBytes\":{},\"availableBytes\":{},\"totalBytes\":{},\"cachedBytes\":{},\"percent\":{:.1},\"swapUsedBytes\":{},\"swapTotalBytes\":{},\"swapPercent\":{:.1}}},\"gpu\":{{\"available\":{},\"vendor\":{},\"name\":{},\"percent\":{:.1},\"vramUsedBytes\":{},\"vramTotalBytes\":{},\"vramPercent\":{:.1},\"tempC\":{},\"hotspotTempC\":{},\"memTempC\":{},\"powerWatts\":{},\"powerCapWatts\":{},\"clockMhz\":{},\"memClockMhz\":{},\"fanRpm\":{}}},\"network\":{{\"interface\":{},\"connected\":{},\"kind\":{},\"ip\":{},\"linkSpeedMbps\":{},\"rxBytesPerSec\":{:.1},\"txBytesPerSec\":{:.1},\"rxTotalBytes\":{},\"txTotalBytes\":{}}},\"topProcesses\":[{}]}}\n",
        now_unix as u64,
        json_escape(&cache.hostname),
        uptime_sec,
        json_escape(&cache.cpu_model),
        cache.cpu_cores,
        cache.cpu_threads,
        cpu_pct,
        per_core_json.join(","),
        opt_f64_json(cpu_temp, 1),
        avg_freq,
        max_freq,
        load1,
        load5,
        load15,
        mem.used,
        mem.avail,
        mem.total,
        mem.cached,
        mem.pct,
        mem.swap_used,
        mem.swap_total,
        mem.swap_pct,
        gpu.available,
        json_escape(&gpu.vendor),
        json_escape(&gpu.name),
        gpu.percent,
        gpu.vram_used,
        gpu.vram_total,
        gpu.vram_pct,
        opt_f64_json(gpu.temp_c, 1),
        opt_f64_json(gpu.hotspot_temp_c, 1),
        opt_f64_json(gpu.mem_temp_c, 1),
        opt_f64_json(gpu.power_w, 1),
        opt_f64_json(gpu.power_cap_w, 0),
        opt_u64_json(gpu.clock_mhz),
        opt_u64_json(gpu.mem_clock_mhz),
        opt_u64_json(gpu.fan_rpm),
        json_escape(if iface.is_empty() { "offline" } else { &iface }),
        connected,
        json_escape(net_kind),
        json_escape(&ip_addr),
        opt_u64_json(link_speed),
        rx_rate,
        tx_rate,
        rx_total,
        tx_total,
        top_procs_json.join(",")
    );

    let mut stdout = io::stdout().lock();
    if stdout.write_all(json.as_bytes()).is_err() || stdout.flush().is_err() {
        std::process::exit(0);
    }

    PrevState {
        instant: Instant::now(),
        cpus: cur_cpus,
        net: cur_net,
        procs: cur_procs,
        total_jiffies: cur_jiffies,
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut cache = StaticCache::new();

    if args.get(1).map(|s| s.as_str()) == Some("--watch") {
        let interval_sec = args
            .get(2)
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(2.0)
            .clamp(0.5, 60.0);
        let interval = Duration::from_secs_f64(interval_sec);

        let mut state: Option<PrevState> = None;
        loop {
            if parent_is_init() {
                break;
            }
            state = Some(collect_and_emit(&mut cache, state));
            thread::sleep(interval);
        }
    } else {
        let _ = collect_and_emit(&mut cache, None);
    }
}

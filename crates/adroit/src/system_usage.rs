//! Whole-machine usage, sampled away from the UI thread.

use crate::icons::Icon;
use crate::settings::UsageSettings;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub cpu: Option<f32>,
    pub gpu: Option<f32>,
    pub memory: Option<f32>,
    pub storage: Option<f32>,
    pub cpu_note: String,
    pub gpu_note: String,
    pub memory_note: String,
    pub storage_note: String,
}

pub struct Gauge {
    pub label: &'static str,
    pub icon: Icon,
    pub class: String,
    pub value: String,
    pub used: f32,
    pub note: String,
}

impl Snapshot {
    pub fn gauges(&self, settings: &UsageSettings) -> Vec<Gauge> {
        [
            (
                "CPU",
                "cpu",
                Icon::Cpu,
                settings.cpu,
                self.cpu,
                &self.cpu_note,
            ),
            (
                "GPU",
                "gpu",
                Icon::Gpu,
                settings.gpu,
                self.gpu,
                &self.gpu_note,
            ),
            (
                "Memory",
                "memory",
                Icon::Memory,
                settings.memory,
                self.memory,
                &self.memory_note,
            ),
            (
                "Storage",
                "storage",
                Icon::Database,
                settings.storage,
                self.storage,
                &self.storage_note,
            ),
        ]
        .into_iter()
        .filter(|(_, _, _, enabled, _, _)| *enabled)
        .map(|(label, kind, icon, _, used, note)| Gauge {
            label,
            icon,
            class: format!(
                "usage-resource {kind}{}",
                match used {
                    Some(n) if n > 95.0 => " critical",
                    Some(n) if n > 90.0 => " warning",
                    _ => "",
                }
            ),
            value: used.map_or_else(|| "N/A".into(), |n| format!("{:.0}%", n.round())),
            used: used.unwrap_or(0.0),
            note: note.clone(),
        })
        .collect()
    }
}

fn percent(used: u64, total: u64) -> Option<f32> {
    (total > 0).then(|| (used as f64 / total as f64 * 100.0).clamp(0.0, 100.0) as f32)
}

fn capacity_note(used: u64, total: u64) -> String {
    if total == 0 {
        return String::new();
    }
    fn amount(bytes: u64) -> (String, &'static str) {
        let units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
        let mut unit = 0;
        let mut value = bytes as f64;
        while value >= 1024.0 && unit < units.len() - 1 {
            value /= 1024.0;
            unit += 1;
        }
        let value = if unit == 0 {
            format!("{value:.0}")
        } else {
            format!("{value:.1}")
        };
        (value, units[unit])
    }
    let (used, used_unit) = amount(used);
    let (total, total_unit) = amount(total);
    if used_unit == total_unit {
        format!("{used} / {total} {total_unit}")
    } else {
        format!("{used} {used_unit} / {total} {total_unit}")
    }
}

fn hardware_note(model: &str, count: usize, unit: &str) -> String {
    let model = model.trim();
    let count = if count > 0 {
        format!("{count} {unit}{}", if count == 1 { "" } else { "s" })
    } else {
        String::new()
    };
    [model, &count]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

#[derive(Default)]
pub struct Sampler {
    system: sysinfo::System,
    cpu_ready: bool,
}

impl Sampler {
    pub fn sample(&mut self, settings: UsageSettings) -> Snapshot {
        let cpu = if settings.cpu {
            self.system.refresh_cpu_usage();
            let ready = std::mem::replace(&mut self.cpu_ready, true);
            ready.then(|| self.system.global_cpu_usage().clamp(0.0, 100.0))
        } else {
            self.cpu_ready = false;
            None
        };
        let (memory, memory_note) = if settings.memory {
            self.system.refresh_memory();
            let (used, total) = (self.system.used_memory(), self.system.total_memory());
            (percent(used, total), capacity_note(used, total))
        } else {
            (None, String::new())
        };
        let (storage, storage_note) = if settings.storage {
            storage_capacity()
                .map(|(used, total)| (percent(used, total), capacity_note(used, total)))
                .unwrap_or_default()
        } else {
            (None, String::new())
        };
        let cpu_note = if settings.cpu {
            hardware_note(
                self.system.cpus().first().map_or("", |cpu| cpu.brand()),
                self.system.cpus().len(),
                "thread",
            )
        } else {
            String::new()
        };
        Snapshot {
            cpu,
            memory,
            storage,
            cpu_note,
            memory_note,
            storage_note,
            ..Snapshot::default()
        }
    }
}

// fsblkcnt_t is u32 on macOS and u64 on Linux.
#[cfg(unix)]
#[allow(clippy::unnecessary_cast)]
fn storage_capacity() -> Option<(u64, u64)> {
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: the path is NUL-terminated and stats points to writable storage.
    if unsafe { libc::statvfs(c"/".as_ptr(), stats.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: statvfs initialized stats on success.
    let stats = unsafe { stats.assume_init() };
    let block_size = if stats.f_frsize > 0 {
        stats.f_frsize
    } else {
        stats.f_bsize
    } as u64;
    let total = (stats.f_blocks as u64).checked_mul(block_size)?;
    let used = (stats.f_blocks as u64)
        .saturating_sub(stats.f_bavail as u64)
        .checked_mul(block_size)?;
    (total > 0).then_some((used, total))
}

#[cfg(not(unix))]
fn storage_capacity() -> Option<(u64, u64)> {
    None
}

#[cfg(any(target_os = "macos", test))]
fn gpu_percent(report: &str) -> Option<f32> {
    report
        .split("\"Device Utilization %\"")
        .skip(1)
        .filter_map(|rest| {
            let rest = rest.trim_start().strip_prefix('=')?.trim_start();
            let value: f32 = rest
                .split(|c: char| !c.is_ascii_digit() && c != '.')
                .next()?
                .parse()
                .ok()?;
            (value.is_finite() && (0.0..=100.0).contains(&value)).then_some(value)
        })
        .max_by(f32::total_cmp)
}

#[derive(Default)]
pub struct GpuSample {
    pub used: Option<f32>,
    pub note: String,
}

#[cfg(any(target_os = "macos", test))]
fn gpu_sample(report: &str) -> GpuSample {
    fn property<'a>(report: &'a str, key: &str) -> Option<&'a str> {
        let key = format!("\"{key}\"");
        report.lines().find_map(|line| {
            line.split_once(&key)?
                .1
                .trim_start()
                .strip_prefix('=')
                .map(str::trim)
        })
    }
    report
        .split("+-o ")
        .map(|device| {
            let model = property(device, "model")
                .and_then(|s| s.strip_prefix('"'))
                .and_then(|s| s.split('"').next())
                .unwrap_or("");
            let cores = property(device, "gpu-core-count")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            GpuSample {
                used: gpu_percent(device),
                note: hardware_note(model, cores, "core"),
            }
        })
        .filter(|sample| sample.used.is_some() || !sample.note.is_empty())
        .max_by(|a, b| a.used.unwrap_or(-1.0).total_cmp(&b.used.unwrap_or(-1.0)))
        .unwrap_or_default()
}

pub async fn gpu_usage() -> GpuSample {
    #[cfg(target_os = "macos")]
    {
        let mut command = tokio::process::Command::new("/usr/sbin/ioreg");
        command
            .args(["-r", "-c", "IOAccelerator", "-l"])
            .kill_on_drop(true);
        if let Ok(Ok(output)) =
            tokio::time::timeout(std::time::Duration::from_secs(1), command.output()).await
            && output.status.success()
        {
            return gpu_sample(&String::from_utf8_lossy(&output.stdout));
        }
    }
    GpuSample::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn live_samples_prime_cpu_counters_and_read_memory_and_startup_capacity() {
        let mut sampler = Sampler::default();
        let settings = UsageSettings::default();
        let first = sampler.sample(settings);
        assert!(first.cpu.is_none());
        assert!(first.memory.is_some_and(|n| (0.0..=100.0).contains(&n)));
        assert!(first.storage.is_some_and(|n| (0.0..=100.0).contains(&n)));
        assert!(first.cpu_note.contains("thread"));
        assert!(first.memory_note.contains(" / "));
        assert!(first.storage_note.contains(" / "));
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        let second = sampler.sample(settings);
        assert!(second.cpu.is_some_and(|n| (0.0..=100.0).contains(&n)));
        assert_eq!(
            sampler.sample(UsageSettings {
                cpu: false,
                gpu: false,
                memory: false,
                storage: false,
            }),
            Snapshot::default()
        );
        assert!(sampler.sample(settings).cpu.is_none());
    }

    #[test]
    fn resource_colors_remain_distinct_until_usage_exceeds_warning_thresholds() {
        let settings = UsageSettings::default();
        let snapshot = Snapshot {
            cpu: Some(90.0),
            gpu: Some(90.01),
            memory: Some(95.0),
            storage: Some(95.01),
            ..Snapshot::default()
        };
        let gauges = snapshot.gauges(&settings);
        assert_eq!(
            gauges.iter().map(|g| g.class.as_str()).collect::<Vec<_>>(),
            [
                "usage-resource cpu",
                "usage-resource gpu warning",
                "usage-resource memory warning",
                "usage-resource storage critical",
            ]
        );
        let normal = Snapshot {
            cpu: Some(20.0),
            gpu: Some(30.0),
            memory: Some(40.0),
            storage: Some(50.0),
            ..Snapshot::default()
        };
        assert_eq!(
            normal
                .gauges(&settings)
                .iter()
                .map(|g| g.class.as_str())
                .collect::<Vec<_>>(),
            [
                "usage-resource cpu",
                "usage-resource gpu",
                "usage-resource memory",
                "usage-resource storage",
            ]
        );
        assert!(
            Snapshot::default()
                .gauges(&settings)
                .iter()
                .all(|g| !g.class.contains("warning") && !g.class.contains("critical"))
        );
    }

    #[test]
    fn capacity_notes_keep_useful_precision_with_binary_units() {
        assert_eq!(
            capacity_note(34 * 1024_u64.pow(3), 64 * 1024_u64.pow(3)),
            "34.0 / 64.0 GiB"
        );
        assert_eq!(
            capacity_note(750 * 1024_u64.pow(3), 2 * 1024_u64.pow(4)),
            "750.0 GiB / 2.0 TiB"
        );
        assert_eq!(capacity_note(0, 512), "0 / 512 B");
        assert_eq!(capacity_note(0, 0), "");
    }

    #[test]
    fn hardware_notes_keep_available_model_and_processor_counts() {
        assert_eq!(
            hardware_note("Apple M1 Max", 10, "thread"),
            "Apple M1 Max · 10 threads"
        );
        assert_eq!(hardware_note("  ", 1, "core"), "1 core");
        assert_eq!(hardware_note("Apple M1 Max", 0, "core"), "Apple M1 Max");
        assert_eq!(hardware_note("", 0, "thread"), "");
    }

    #[test]
    fn graphics_notes_describe_the_device_with_the_reported_usage() {
        let report = r#"+-o Integrated
  | "model" = "Integrated GPU"
  | "gpu-core-count" = 8
  | "PerformanceStatistics" = {"Device Utilization %"=25}
+-o Discrete
  | "model" = "Discrete GPU"
  | "gpu-core-count" = 32
  | "PerformanceStatistics" = {"Device Utilization %"=75}"#;
        let sample = gpu_sample(report);
        assert_eq!(sample.used, Some(75.0));
        assert_eq!(sample.note, "Discrete GPU · 32 cores");
        let missing = gpu_sample("");
        assert_eq!(missing.used, None);
        assert!(missing.note.is_empty());
        let model_only = gpu_sample(r#""model" = "GPU""#);
        assert_eq!(model_only.used, None);
        assert_eq!(model_only.note, "GPU");
    }

    #[test]
    fn gauges_include_their_own_supplementary_information() {
        let snapshot = Snapshot {
            cpu_note: "CPU details".into(),
            gpu_note: "GPU details".into(),
            memory_note: "34.0 / 64.0 GiB".into(),
            storage_note: "700.0 / 950.0 GiB".into(),
            ..Snapshot::default()
        };
        let gauges = snapshot.gauges(&UsageSettings::default());
        assert_eq!(
            gauges.iter().map(|g| g.note.as_str()).collect::<Vec<_>>(),
            [
                "CPU details",
                "GPU details",
                "34.0 / 64.0 GiB",
                "700.0 / 950.0 GiB"
            ]
        );
        assert!(
            Snapshot::default()
                .gauges(&UsageSettings::default())
                .iter()
                .all(|g| g.note.is_empty())
        );
    }

    #[test]
    fn percentages_require_a_capacity_and_stay_within_the_gauge() {
        assert_eq!(percent(25, 100), Some(25.0));
        assert_eq!(percent(1, 0), None);
        assert_eq!(percent(110, 100), Some(100.0));
    }

    #[test]
    fn gpu_usage_uses_the_busiest_device_without_combining_renderer_and_tiler() {
        let report = r#""PerformanceStatistics" = {"Tiler Utilization %"=99,"Device Utilization %"=24,"Renderer Utilization %"=80}
"PerformanceStatistics" = {"Device Utilization %"=42}"#;
        assert_eq!(gpu_percent(report), Some(42.0));
        assert_eq!(
            gpu_percent(r#""PerformanceStatistics" = {"Tiler Utilization %"=99}"#),
            None
        );
        assert_eq!(gpu_percent(r#""Device Utilization %"=0"#), Some(0.0));
        assert_eq!(gpu_percent(r#""Device Utilization %"=101"#), None);
        assert_eq!(gpu_percent("unavailable"), None);
    }

    #[test]
    fn visibility_is_independent_of_resource_availability() {
        let snapshot = Snapshot {
            cpu: Some(12.5),
            gpu: None,
            memory: Some(50.0),
            storage: Some(75.0),
            ..Snapshot::default()
        };
        let settings = crate::settings::UsageSettings::default();
        let gauges = snapshot.gauges(&settings);
        assert_eq!(
            gauges.iter().map(|g| g.label).collect::<Vec<_>>(),
            ["CPU", "GPU", "Memory", "Storage"]
        );
        assert_eq!(
            gauges.iter().map(|g| g.icon.name()).collect::<Vec<_>>(),
            ["cpu", "gpu", "memory", "database"]
        );
        assert_eq!(gauges[0].value, "13%");
        assert_eq!(gauges[1].value, "N/A");
        assert_eq!(gauges[1].used, 0.0);
        assert!(
            snapshot
                .gauges(&crate::settings::UsageSettings {
                    cpu: false,
                    gpu: false,
                    memory: false,
                    storage: false
                })
                .is_empty()
        );
        let only_memory = snapshot.gauges(&crate::settings::UsageSettings {
            cpu: false,
            gpu: false,
            memory: true,
            storage: false,
        });
        assert_eq!(only_memory.len(), 1);
        assert_eq!(only_memory[0].label, "Memory");
    }
}

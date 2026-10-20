//! Whole-machine usage, sampled away from the UI thread.

use crate::icons::Icon;
use crate::settings::UsageSettings;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub cpu: Option<f32>,
    pub gpu: Option<f32>,
    pub memory: Option<f32>,
    pub storage: Option<f32>,
}

pub struct Gauge {
    pub label: &'static str,
    pub icon: Icon,
    pub class: String,
    pub value: String,
    pub used: f32,
}

impl Snapshot {
    pub fn gauges(&self, settings: &UsageSettings) -> Vec<Gauge> {
        [
            ("CPU", "cpu", Icon::Cpu, settings.cpu, self.cpu),
            ("GPU", "gpu", Icon::Gpu, settings.gpu, self.gpu),
            (
                "Memory",
                "memory",
                Icon::Memory,
                settings.memory,
                self.memory,
            ),
            (
                "Storage",
                "storage",
                Icon::Database,
                settings.storage,
                self.storage,
            ),
        ]
        .into_iter()
        .filter(|(_, _, _, enabled, _)| *enabled)
        .map(|(label, kind, icon, _, used)| Gauge {
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
        })
        .collect()
    }
}

fn percent(used: u64, total: u64) -> Option<f32> {
    (total > 0).then(|| (used as f64 / total as f64 * 100.0).clamp(0.0, 100.0) as f32)
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
        let memory = if settings.memory {
            self.system.refresh_memory();
            percent(self.system.used_memory(), self.system.total_memory())
        } else {
            None
        };
        Snapshot {
            cpu,
            memory,
            storage: settings.storage.then(storage_percent).flatten(),
            gpu: None,
        }
    }
}

// fsblkcnt_t is u32 on macOS and u64 on Linux.
#[cfg(unix)]
#[allow(clippy::unnecessary_cast)]
fn storage_percent() -> Option<f32> {
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: the path is NUL-terminated and stats points to writable storage.
    if unsafe { libc::statvfs(c"/".as_ptr(), stats.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: statvfs initialized stats on success.
    let stats = unsafe { stats.assume_init() };
    percent(
        (stats.f_blocks as u64).saturating_sub(stats.f_bavail as u64),
        stats.f_blocks as u64,
    )
}

#[cfg(not(unix))]
fn storage_percent() -> Option<f32> {
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

pub async fn gpu_usage() -> Option<f32> {
    #[cfg(target_os = "macos")]
    {
        let mut command = tokio::process::Command::new("/usr/sbin/ioreg");
        command
            .args(["-r", "-c", "IOAccelerator", "-l"])
            .kill_on_drop(true);
        let output = tokio::time::timeout(std::time::Duration::from_secs(1), command.output())
            .await
            .ok()?
            .ok()?;
        if output.status.success() {
            return gpu_percent(&String::from_utf8_lossy(&output.stdout));
        }
    }
    None
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

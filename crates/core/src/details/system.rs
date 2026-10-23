//! The whole machine's usage as the foot of the details panel draws it.

use adroit_protocol::SystemUsage;

use crate::icons::Icon;
use crate::settings::UsageSettings;

#[derive(Debug, Clone, PartialEq)]
/// A resource of the machine as the foot of the details panel draws it.
pub struct SystemGauge {
    pub label: &'static str,
    pub icon: Icon,
    pub class: String,
    pub value: String,
    pub used: f32,
    pub note: String,
}

/// The gauges of the resources `settings` show, from `usage`.
pub fn system_gauges(usage: &SystemUsage, settings: &UsageSettings) -> Vec<SystemGauge> {
    [
        (
            "CPU",
            "cpu",
            Icon::Cpu,
            settings.cpu,
            usage.cpu,
            &usage.cpu_note,
        ),
        (
            "GPU",
            "gpu",
            Icon::Gpu,
            settings.gpu,
            usage.gpu,
            &usage.gpu_note,
        ),
        (
            "Memory",
            "memory",
            Icon::Memory,
            settings.memory,
            usage.memory,
            &usage.memory_note,
        ),
        (
            "Storage",
            "storage",
            Icon::Database,
            settings.storage,
            usage.storage,
            &usage.storage_note,
        ),
    ]
    .into_iter()
    .filter(|(_, _, _, enabled, _, _)| *enabled)
    .map(|(label, kind, icon, _, used, note)| SystemGauge {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_colors_remain_distinct_until_usage_exceeds_warning_thresholds() {
        let settings = UsageSettings::default();
        let snapshot = SystemUsage {
            cpu: Some(90.0),
            gpu: Some(90.01),
            memory: Some(95.0),
            storage: Some(95.01),
            ..SystemUsage::default()
        };
        let gauges = system_gauges(&snapshot, &settings);
        assert_eq!(
            gauges.iter().map(|g| g.class.as_str()).collect::<Vec<_>>(),
            [
                "usage-resource cpu",
                "usage-resource gpu warning",
                "usage-resource memory warning",
                "usage-resource storage critical",
            ]
        );
        let normal = SystemUsage {
            cpu: Some(20.0),
            gpu: Some(30.0),
            memory: Some(40.0),
            storage: Some(50.0),
            ..SystemUsage::default()
        };
        assert_eq!(
            system_gauges(&normal, &settings)
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
            system_gauges(&SystemUsage::default(), &settings)
                .iter()
                .all(|g| !g.class.contains("warning") && !g.class.contains("critical"))
        );
    }

    #[test]
    fn gauges_include_their_own_supplementary_information() {
        let snapshot = SystemUsage {
            cpu_note: "CPU details".into(),
            gpu_note: "GPU details".into(),
            memory_note: "34.0 / 64.0 GiB".into(),
            storage_note: "700.0 / 950.0 GiB".into(),
            ..SystemUsage::default()
        };
        let gauges = system_gauges(&snapshot, &UsageSettings::default());
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
            system_gauges(&SystemUsage::default(), &UsageSettings::default())
                .iter()
                .all(|g| g.note.is_empty())
        );
    }

    #[test]
    fn visibility_is_independent_of_resource_availability() {
        let snapshot = SystemUsage {
            cpu: Some(12.5),
            gpu: None,
            memory: Some(50.0),
            storage: Some(75.0),
            ..SystemUsage::default()
        };
        let settings = UsageSettings::default();
        let gauges = system_gauges(&snapshot, &settings);
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
            system_gauges(
                &snapshot,
                &UsageSettings {
                    cpu: false,
                    gpu: false,
                    memory: false,
                    storage: false
                }
            )
            .is_empty()
        );
        let only_memory = system_gauges(
            &snapshot,
            &UsageSettings {
                cpu: false,
                gpu: false,
                memory: true,
                storage: false,
            },
        );
        assert_eq!(only_memory.len(), 1);
        assert_eq!(only_memory[0].label, "Memory");
    }
}

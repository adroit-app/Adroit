//! The whole machine's usage, for the foot of the details panel.

use std::time::Duration;

use adroit_protocol::{DetailsChange, Sight};

use super::Watched;
use crate::settings::UsageSettings;
use crate::system_usage::{Sampler, gpu_usage};

pub(super) async fn watch(watched: Watched) {
    let mut sampler = Sampler::default();
    loop {
        let settings = sampled(&watched.sights(), crate::settings::current().usage);
        if settings.any() {
            let sampled = crate::environment::spawn_blocking(move || {
                let usage = sampler.sample(settings);
                (sampler, usage)
            })
            .await;
            match sampled {
                Ok((next, mut usage)) => {
                    sampler = next;
                    if settings.gpu {
                        let gpu = gpu_usage().await;
                        usage.gpu = gpu.used;
                        usage.gpu_note = gpu.note;
                    }
                    watched.record([DetailsChange::System { usage }]);
                }
                Err(_) => sampler = Sampler::default(),
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// What to sample for `sights`: every resource a window with the details
/// open shows, `own` standing for those of a window that does not say.
fn sampled(sights: &[Sight], own: UsageSettings) -> UsageSettings {
    sights
        .iter()
        .filter(|s| s.details)
        .map(|s| s.system.unwrap_or(own))
        .fold(UsageSettings::NONE, UsageSettings::or)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only_memory() -> UsageSettings {
        UsageSettings {
            memory: true,
            ..UsageSettings::NONE
        }
    }

    #[test]
    fn nothing_is_sampled_while_no_window_shows_the_details() {
        let closed = Sight {
            system: Some(UsageSettings::default()),
            ..Sight::default()
        };
        assert_eq!(
            sampled(&[closed], UsageSettings::default()),
            UsageSettings::NONE
        );
    }

    #[test]
    fn what_a_window_shows_is_sampled_whatever_the_host_s_own_settings_say() {
        let shown = Sight {
            details: true,
            system: Some(UsageSettings::default()),
            ..Sight::default()
        };
        assert_eq!(
            sampled(&[shown], UsageSettings::NONE),
            UsageSettings::default()
        );
    }

    #[test]
    fn every_resource_some_window_shows_is_sampled_and_the_host_s_own_stand_for_a_window_that_does_not_say()
     {
        let cpu = Sight {
            details: true,
            system: Some(UsageSettings {
                cpu: true,
                ..UsageSettings::NONE
            }),
            ..Sight::default()
        };
        let silent = Sight {
            details: true,
            ..Sight::default()
        };
        assert_eq!(
            sampled(&[cpu, silent], only_memory()),
            UsageSettings {
                cpu: true,
                memory: true,
                ..UsageSettings::NONE
            }
        );
    }
}

//! The whole machine's usage, for the foot of the details panel.

use std::time::Duration;

use adroit_protocol::DetailsChange;

use super::Watched;
use crate::system_usage::{Sampler, gpu_usage};

pub(super) async fn watch(watched: Watched) {
    let mut sampler = Sampler::default();
    loop {
        let settings = crate::settings::current().usage;
        if watched.sight().details && settings.any() {
            let sampled = tokio::task::spawn_blocking(move || {
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

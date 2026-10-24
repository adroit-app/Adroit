//! How much of the account's usage limits is left, for the foot of the
//! details panel: asked of the agent shown while the panel is open, again
//! once the answer has aged.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use adroit_agents::claude::usage::Fresh;
use adroit_protocol::DetailsChange;

use super::{Watched, distinct};

pub(super) async fn watch(mut watched: Watched) {
    let mut fresh = HashMap::<adroit_model::Agent, Fresh>::new();
    loop {
        for agent in distinct(&watched.sights(), |s| s.usage.filter(|_| s.details)) {
            let held = fresh.entry(agent).or_default();
            if held.want(Instant::now()) {
                let got = adroit_agents::adapter(agent)
                    .usage(crate::programs::context(agent), crate::paths::home())
                    .await;
                held.settle(Instant::now(), got);
                if let Some(limits) = held.get() {
                    watched.record([DetailsChange::Usage {
                        agent,
                        limits: limits.to_vec(),
                    }]);
                }
            }
        }
        watched
            .wait(Duration::from_secs(30), |seen| {
                seen.iter()
                    .map(|s| (s.sight.details, s.sight.usage))
                    .collect::<Vec<_>>()
            })
            .await;
    }
}

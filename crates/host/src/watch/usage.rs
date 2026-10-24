//! How much of the account's usage limits is left, for the foot of the
//! details panel: asked of the agent shown while the panel is open, again
//! once the answer has aged.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use adroit_agents::claude::usage::Fresh;
use adroit_protocol::DetailsChange;

use super::{Watched, distinct};

pub(super) async fn watch(mut watched: Watched) {
    // By the login, which is the account the limits are of.
    let mut fresh = HashMap::<adroit_protocol::Account, Fresh>::new();
    loop {
        let shown = distinct(&watched.sights(), |s| s.usage.clone().filter(|_| s.details));
        for account in shown {
            let held = fresh.entry(account.clone()).or_default();
            if held.want(Instant::now()) {
                let got =
                    match super::asked_in(&watched.seen(), account.agent, account.home.as_deref())
                        .await
                    {
                        Some(context) => {
                            adroit_agents::adapter(account.agent)
                                .usage(context, crate::paths::home())
                                .await
                        }
                        None => None,
                    };
                held.settle(Instant::now(), got);
            }
            if let Some(limits) = held.get() {
                watched.record([DetailsChange::Usage {
                    account,
                    limits: limits.to_vec(),
                }]);
            }
        }
        watched
            .wait(Duration::from_secs(30), |seen| {
                seen.iter()
                    .map(|s| (s.sight.details, s.sight.usage.clone()))
                    .collect::<Vec<_>>()
            })
            .await;
    }
}

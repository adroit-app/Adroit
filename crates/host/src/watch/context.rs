//! How full each shown conversation's context is, for an agent whose
//! transcript does not say: asked again as the conversation moves on.

use std::time::{Duration, Instant};

use adroit_agents::claude::context::Fresh;
use adroit_protocol::{ContextRead, DetailsChange};

use super::{Watched, shown_sessions};

pub(super) async fn watch(watched: Watched) {
    let mut asked = Fresh::<(String, String)>::default();
    loop {
        let shown = shown_sessions(&watched.seen(), |s| s.details)
            .into_iter()
            .map(|(s, _)| s)
            .filter(|s| adroit_agents::adapter(s.agent).capabilities().usage_limits);
        for s in shown {
            let stats = watched
                .host()
                .work()
                .details()
                .sessions
                .get(&s.id)
                .and_then(|read| read.conversation.as_ref())
                .filter(|c| c.id == s.session_id)
                .map(|c| c.stats.clone());
            let Some(stats) = stats.filter(|stats| stats.context.is_none()) else {
                continue;
            };
            let key = (s.id.clone(), s.session_id.clone());
            let mark = stats.tokens.map_or(0, |t| t.total());
            if asked.want(&key, mark, Instant::now()) {
                let Ok(started) = crate::programs::of_session(&s).await else {
                    continue;
                };
                let got = adroit_agents::adapter(s.agent)
                    .context(started, &s, stats.model)
                    .await;
                asked.settle(&key, Instant::now(), got);
            }
            if let Some(context) = asked.get(&key) {
                watched.record([DetailsChange::Context {
                    session: s.id.clone(),
                    context: ContextRead {
                        conversation: s.session_id.clone(),
                        context,
                    },
                }]);
            }
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

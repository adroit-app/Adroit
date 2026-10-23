//! How full the shown conversation's context is, for an agent whose
//! transcript does not say: asked again as the conversation moves on.

use std::time::{Duration, Instant};

use adroit_agents::claude::context::Fresh;
use adroit_protocol::{ContextRead, DetailsChange};

use super::Watched;

pub(super) async fn watch(watched: Watched) {
    let mut asked = Fresh::<(String, String)>::default();
    loop {
        let seen = watched.seen();
        let shown = seen.session.filter(|s| {
            seen.sight.details && adroit_agents::adapter(s.agent).capabilities().usage_limits
        });
        let reading = shown.and_then(|s| {
            let stats = watched
                .host()
                .work()
                .details()
                .sessions
                .get(&s.id)?
                .conversation
                .as_ref()
                .filter(|c| c.id == s.session_id)?
                .stats
                .clone();
            stats.context.is_none().then_some((s, stats))
        });
        if let Some((s, stats)) = reading {
            let key = (s.id.clone(), s.session_id.clone());
            let mark = stats.tokens.map_or(0, |t| t.total());
            if asked.want(&key, mark, Instant::now()) {
                let got = adroit_agents::adapter(s.agent)
                    .context(crate::programs::agents(), &s, stats.model)
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

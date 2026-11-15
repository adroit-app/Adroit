//! The models each configured profile's agent offers, asked again as the
//! profiles change and while they go unanswered.
use std::{collections::HashMap, time::Duration};

use super::Context;

pub(super) async fn watch_models(context: Context) {
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // By the generation each profile is asked as, so a profile configured
    // anew is asked at once and an answer is learned for the one asked.
    let mut next: HashMap<u64, tokio::time::Instant> = HashMap::new();
    let mut asking = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _ = interval.tick() => {}
            Some(Ok((generation, got))) = asking.join_next(), if !asking.is_empty() => {
                let got: Option<Vec<adroit_model::ProviderModel>> = got;
                let wait = if got.is_some() { 600 } else { 60 };
                next.insert(generation, tokio::time::Instant::now() + Duration::from_secs(wait));
                if let Some(models) = got {
                    context.models.lock().expect("the models' lock").learn(generation, models);
                }
                continue;
            }
        }
        let Ok(settings) = context.settings() else {
            continue;
        };
        let profiles = crate::programs::profiles(&settings);
        let to_ask = {
            let mut models = context.models.lock().expect("the models' lock");
            models.configure(
                profiles.iter().map(asked_of).collect(),
                settings.profile_of(settings.default_agent()).id.clone(),
            );
            models.to_ask()
        };
        let now = tokio::time::Instant::now();
        next.retain(|generation, _| to_ask.iter().any(|(_, g)| g == generation));
        for (asked, generation) in to_ask {
            let Some(started) = profiles.iter().find(|p| p.profile == asked.profile) else {
                continue;
            };
            if next.get(&generation).is_none_or(|at| now >= *at) {
                // Far ahead while asking, so a slow answer is not asked twice.
                next.insert(generation, now + Duration::from_secs(600));
                let answer = adroit_agents::adapter(started.agent())
                    .models(started.clone(), crate::paths::home());
                asking.spawn(async move { (generation, answer.await) });
            }
        }
    }
}

/// What `started`'s models are asked through, as the catalog keeps it.
fn asked_of(started: &adroit_agents::AgentContext) -> adroit_model::Asked {
    adroit_model::Asked {
        profile: started.profile.clone(),
        agent: started.agent(),
        home: started.home.clone(),
        through: format!("{:?}", (&started.home, &started.launch, &started.shell)),
    }
}

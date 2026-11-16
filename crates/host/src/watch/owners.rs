//! What the picker asks of GitHub while a window shows it: the owners `gh`
//! is logged in as, and the repositories of each owner typed, each read
//! once while the picker stays open.

use std::collections::BTreeSet;
use std::time::Duration;

use adroit_protocol::{PickChange, Sight};
use adroit_vcs::github;

use super::Watched;

pub(super) async fn watch(mut watched: Watched) {
    let mut read = Asked::default();
    loop {
        let now = asked(watched.seen().iter().map(|s| &s.sight));
        if !now.picking {
            read = Asked::default();
        }
        if now.accounts && !read.accounts {
            // What failed is not asked again until the picker opens anew, so
            // a `gh` not logged in is not run on every key typed.
            read.accounts = true;
            match github::accounts().await {
                Ok(owners) => watched.record([PickChange::Accounts { owners }]),
                Err(error) => tracing::debug!(%error, "gh listed no accounts"),
            }
        }
        for owner in now
            .owners
            .difference(&read.owners)
            .cloned()
            .collect::<Vec<_>>()
        {
            read.owners.insert(owner.clone());
            match github::repositories(&owner).await {
                Ok(repositories) => watched.record([PickChange::Owned {
                    owner,
                    repositories,
                }]),
                Err(error) => tracing::debug!(%owner, %error, "gh listed no repositories"),
            }
        }
        watched
            .wait(Duration::from_secs(300), |seen| {
                asked(seen.iter().map(|s| &s.sight))
            })
            .await;
    }
}

/// What every window's picker asks of GitHub.
#[derive(Debug, Default, PartialEq, Eq)]
struct Asked {
    picking: bool,
    accounts: bool,
    owners: BTreeSet<String>,
}

fn asked<'a>(sights: impl Iterator<Item = &'a Sight>) -> Asked {
    let mut asked = Asked::default();
    for pick in sights.filter_map(|s| s.pick.as_ref()) {
        asked.picking = true;
        asked.accounts |= pick.accounts;
        asked.owners.extend(pick.owners.iter().cloned());
    }
    asked
}

#[cfg(test)]
mod tests {
    use adroit_protocol::PickSight;

    use super::*;

    #[test]
    fn what_every_picker_asks_of_github_is_asked_once() {
        let picking = |owners: &[&str], accounts: bool| Sight {
            pick: Some(PickSight {
                owners: owners.iter().map(|o| o.to_string()).collect(),
                accounts,
                ..PickSight::default()
            }),
            ..Sight::default()
        };
        let sights = [
            picking(&["a"], false),
            picking(&["a", "b"], true),
            Sight::default(),
        ];
        assert_eq!(
            asked(sights.iter()),
            Asked {
                picking: true,
                accounts: true,
                owners: ["a".to_string(), "b".to_string()].into(),
            }
        );
        assert_eq!(asked([Sight::default()].iter()), Asked::default());
    }
}

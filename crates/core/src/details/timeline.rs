//! The timeline sheet's rows and the task opened from them.

use std::collections::HashSet;

use adroit_model::timeline::{Revision, Shares, Status, Task, Timeline, Tree};

use super::{Entry, Group, lasted};

/// How a piece of work splits by status, in hundredths of a percent, so
/// that rows compare exactly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Bar {
    pub completed: u16,
    pub in_progress: u16,
    pub blocked: u16,
    pub pending: u16,
}

impl Bar {
    pub fn of(shares: &Shares) -> Bar {
        let part = |share: f64| (share.clamp(0.0, 1.0) * 10_000.0).round() as u16;
        Bar {
            completed: part(shares.completed),
            in_progress: part(shares.in_progress),
            blocked: part(shares.blocked),
            pending: part(shares.pending),
        }
    }

    /// The completed share, as a percentage with two decimals.
    pub fn percent(&self) -> String {
        format!("{}.{:02}%", self.completed / 100, self.completed % 100)
    }

    /// Each status's part, in the order the bar draws them, as a percentage.
    pub fn parts(&self) -> [(Status, f64); 4] {
        let pc = |n: u16| f64::from(n) / 100.0;
        [
            (Status::Completed, pc(self.completed)),
            (Status::InProgress, pc(self.in_progress)),
            (Status::Blocked, pc(self.blocked)),
            (Status::Pending, pc(self.pending)),
        ]
    }
}

/// What a status is called beside a tree or a task.
pub fn status_label(status: Status) -> &'static str {
    match status {
        Status::Pending => "Not started",
        Status::InProgress => "In progress",
        Status::Completed => "Completed",
        Status::Blocked => "Blocked",
    }
}

/// The tone a status is drawn in, as a session's state is.
pub fn status_tone(status: Status) -> &'static str {
    match status {
        Status::Pending => "idle",
        Status::InProgress => "run",
        Status::Completed => "done",
        Status::Blocked => "fail",
    }
}

/// A tree's card, heading its tasks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeCard {
    /// What tells its row apart from the tasks'.
    pub key: String,
    pub id: String,
    pub title: String,
    pub status: Status,
    pub bar: Bar,
    pub leaves_done: usize,
    pub leaves: usize,
    pub started_ms: u64,
    pub ended_ms: Option<u64>,
    /// Whether its tasks are left out.
    pub folded: bool,
}

impl TreeCard {
    fn of(tree: &Tree, folded: bool) -> TreeCard {
        TreeCard {
            key: tree_key(&tree.id),
            id: tree.id.clone(),
            title: tree.title.clone(),
            status: tree.status,
            bar: Bar::of(&tree.shares),
            leaves_done: tree.leaves_done,
            leaves: tree.leaves,
            started_ms: tree.started_ms,
            ended_ms: tree.ended_ms,
            folded,
        }
    }

    /// Its leaf tasks completed and how long it has taken, at `now_ms`.
    pub fn tag(&self, now_ms: u64) -> String {
        let took = self
            .ended_ms
            .unwrap_or(now_ms)
            .saturating_sub(self.started_ms);
        format!(
            "{}/{} leaf tasks · {}",
            self.leaves_done,
            self.leaves,
            lasted(took)
        )
    }
}

/// A task as its tree lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskLine {
    pub id: String,
    pub title: String,
    pub status: Status,
    /// How deep it is below its tree's top: 0 for a step of the top.
    pub depth: usize,
    pub bar: Bar,
}

fn tree_key(id: &str) -> String {
    // No task id starts with a NUL, so no task takes a tree's place.
    format!("\0tree:{id}")
}

/// Whether the tree `card` stands for is folded: a completed one is, until
/// the user unfolds it, and any other is not, until the user folds it.
fn folded(tree: &Tree, toggled: &HashSet<String>) -> bool {
    (tree.status == Status::Completed) != toggled.contains(&tree.id)
}

/// The newest `shown` trees, the newest first, each with its tasks unless it
/// is folded, then a row standing for the rest; `toggled` holds the trees the
/// user folded or unfolded.
pub fn timeline_entries(
    timeline: &Timeline,
    toggled: &HashSet<String>,
    shown: usize,
) -> Vec<Entry> {
    let mut trees = timeline.trees();
    trees.reverse();
    let hidden = trees.len().saturating_sub(shown);
    trees.truncate(shown);
    let mut entries = Vec::new();
    for tree in trees {
        let card = TreeCard::of(&tree, folded(&tree, toggled));
        let shut = card.folded;
        entries.push(Entry::Tree(card));
        if !shut {
            entries.extend(
                timeline
                    .rows(&tree.id)
                    .into_iter()
                    .map(|row| Entry::Task(line(&row.task, row.depth, &row.shares))),
            );
        }
    }
    if hidden > 0 {
        entries.push(Entry::More {
            group: Group::Timeline,
            hidden,
        });
    }
    entries
}

fn line(task: &Task, depth: usize, shares: &Shares) -> TaskLine {
    TaskLine {
        id: task.id.clone(),
        title: task.title.clone(),
        status: task.status,
        depth,
        bar: Bar::of(shares),
    }
}

/// The tree `at` lists a task of, or is the card of.
pub fn tree_at(entries: &[Entry], at: usize) -> Option<&TreeCard> {
    entries.get(..=at)?.iter().rev().find_map(|e| match e {
        Entry::Tree(card) => Some(card),
        _ => None,
    })
}

/// The task opened, with what tells of it.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskDetail {
    pub task: Task,
    /// The title of the tree it is in.
    pub tree: String,
    pub bar: Bar,
    pub leaves_done: usize,
    pub leaves: usize,
    /// The changes made to it, the newest first.
    pub history: Vec<Revision>,
}

/// The task `id` of `timeline`, while there is one.
pub fn task_detail(timeline: &Timeline, id: &str) -> Option<TaskDetail> {
    let task = timeline.task(id)?.clone();
    let mut top = &task;
    while let Some(parent) = top.parent.as_deref().and_then(|p| timeline.task(p)) {
        top = parent;
    }
    let leaves = leaves(timeline, id);
    Some(TaskDetail {
        tree: top.title.clone(),
        bar: Bar::of(&timeline.shares_of(id)),
        leaves_done: leaves
            .iter()
            .filter(|t| t.status == Status::Completed)
            .count(),
        leaves: leaves.len(),
        history: timeline.history(id).cloned().collect(),
        task,
    })
}

fn leaves<'a>(timeline: &'a Timeline, id: &str) -> Vec<&'a Task> {
    let steps: Vec<&Task> = timeline.children(Some(id)).collect();
    if steps.is_empty() {
        return timeline.task(id).into_iter().collect();
    }
    steps
        .into_iter()
        .flat_map(|t| leaves(timeline, &t.id))
        .collect()
}

#[cfg(test)]
mod tests {
    use adroit_model::timeline::{Entry as Call, Op};

    use super::*;

    fn add(id: &str, parent: Option<&str>, status: Status) -> Op {
        Op::Add {
            id: id.into(),
            parent: parent.map(Into::into),
            title: id.to_uppercase(),
            weight: None,
            status: Some(status),
            description: None,
            note: None,
        }
    }

    fn timeline(ops: Vec<Op>) -> Timeline {
        Timeline::replay(&[Call { at_ms: 1_000, ops }])
    }

    fn keys(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .map(|e| match e {
                Entry::Tree(card) => format!("tree {}", card.id),
                Entry::Task(line) => format!("{}{}", "  ".repeat(line.depth), line.id),
                Entry::More { hidden, .. } => format!("more {hidden}"),
                other => panic!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn a_share_reads_as_a_percentage_with_two_decimals() {
        let bar = Bar::of(&Shares {
            completed: 0.425,
            pending: 0.575,
            ..Default::default()
        });
        assert_eq!(bar.percent(), "42.50%");
        assert_eq!(Bar::default().percent(), "0.00%");
        assert_eq!(
            Bar::of(&Shares {
                completed: 1.0,
                ..Default::default()
            })
            .percent(),
            "100.00%"
        );
    }

    #[test]
    fn the_trees_come_newest_first_and_a_completed_one_is_folded() {
        let t = timeline(vec![
            add("old", None, Status::Pending),
            add("old-a", Some("old"), Status::InProgress),
            add("done", None, Status::Pending),
            add("done-a", Some("done"), Status::Completed),
            add("new", None, Status::Pending),
            add("new-a", Some("new"), Status::Pending),
            add("new-a1", Some("new-a"), Status::Pending),
        ]);
        assert_eq!(
            keys(&timeline_entries(&t, &HashSet::new(), usize::MAX)),
            [
                "tree new",
                "new-a",
                "  new-a1",
                "tree done",
                "tree old",
                "old-a",
            ]
        );
    }

    #[test]
    fn a_tree_the_user_toggled_is_folded_or_unfolded_against_its_default() {
        let t = timeline(vec![
            add("open", None, Status::Pending),
            add("open-a", Some("open"), Status::Pending),
            add("done", None, Status::Pending),
            add("done-a", Some("done"), Status::Completed),
        ]);
        let toggled = HashSet::from(["open".to_owned(), "done".to_owned()]);
        assert_eq!(
            keys(&timeline_entries(&t, &toggled, usize::MAX)),
            ["tree done", "done-a", "tree open"]
        );
    }

    #[test]
    fn past_the_trees_shown_a_row_shows_the_rest() {
        let t = timeline(vec![
            add("a", None, Status::Pending),
            add("b", None, Status::Pending),
            add("c", None, Status::Pending),
        ]);
        assert_eq!(
            keys(&timeline_entries(&t, &HashSet::new(), 2)),
            ["tree c", "tree b", "more 1"]
        );
        assert_eq!(
            keys(&timeline_entries(&t, &HashSet::new(), 3)),
            ["tree c", "tree b", "tree a"]
        );
    }

    #[test]
    fn a_tree_s_card_tells_its_leaf_tasks_and_how_long_it_took() {
        let t = timeline(vec![
            add("goal", None, Status::Pending),
            add("a", Some("goal"), Status::Completed),
            add("b", Some("goal"), Status::Pending),
        ]);
        let Some(Entry::Tree(card)) = timeline_entries(&t, &HashSet::new(), 1).into_iter().next()
        else {
            panic!("no card");
        };
        assert_eq!(card.tag(1_000 + 25 * 60_000), "1/2 leaf tasks · 25 min");
    }

    #[test]
    fn a_task_opened_tells_its_tree_its_share_and_its_history_newest_first() {
        let mut t = timeline(vec![
            add("goal", None, Status::Pending),
            add("a", Some("goal"), Status::Pending),
            add("a1", Some("a"), Status::Completed),
            add("a2", Some("a"), Status::Pending),
        ]);
        t.apply(&Call {
            at_ms: 2_000,
            ops: vec![Op::Update {
                id: "a".into(),
                status: Some(Status::InProgress),
                title: None,
                weight: None,
                description: None,
                note: Some("halfway".into()),
            }],
        });
        let d = task_detail(&t, "a").unwrap();
        assert_eq!(d.tree, "GOAL");
        assert_eq!(d.bar.percent(), "50.00%");
        assert_eq!((d.leaves_done, d.leaves), (1, 2));
        assert_eq!(
            d.history.iter().map(|r| r.at_ms).collect::<Vec<_>>(),
            [2_000, 1_000]
        );
        assert_eq!(task_detail(&t, "ghost"), None);
    }
}

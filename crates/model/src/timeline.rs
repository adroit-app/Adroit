//! The timeline a session's agent records of its work: trees of weighted
//! tasks, rebuilt with each task's history by replaying the journal of the
//! calls that changed them.

use serde::{Deserialize, Serialize};

/// How a task stands.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    Pending,
    InProgress,
    Completed,
    Blocked,
}

impl Status {
    pub const ALL: [Status; 4] = [
        Status::Pending,
        Status::InProgress,
        Status::Completed,
        Status::Blocked,
    ];

    /// How it is read out: in the tool's answers and the window alike.
    pub fn word(self) -> &'static str {
        match self {
            Status::Pending => "not started",
            Status::InProgress => "in progress",
            Status::Completed => "completed",
            Status::Blocked => "blocked",
        }
    }

    fn mark(self) -> &'static str {
        match self {
            Status::Pending => "[ ]",
            Status::InProgress => "[>]",
            Status::Completed => "[x]",
            Status::Blocked => "[!]",
        }
    }
}

/// A task as it stands after the journal so far.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    /// The task it is a step of; none for the top of a tree.
    pub parent: Option<String>,
    pub title: String,
    pub status: Status,
    /// How large it is beside its siblings.
    pub weight: f64,
    /// What it will do.
    pub description: Option<String>,
    /// What happened: its result, or why it is blocked.
    pub note: Option<String>,
}

/// One change a call asks of the tasks, applied in the order given.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Op {
    /// Add a task: a new tree without `parent`, a step of `parent` with it.
    Add {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<String>,
        title: String,
        /// Its size beside its siblings; 1 when omitted.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        weight: Option<f64>,
        /// `pending` when omitted.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<Status>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    /// Change what is given of a task, leaving the rest.
    Update {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<Status>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        weight: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    /// Remove a task with every step under it.
    Remove { id: String },
}

impl Op {
    fn id(&self) -> &str {
        match self {
            Op::Add { id, .. } | Op::Update { id, .. } | Op::Remove { id } => id,
        }
    }
}

/// One call as the journal keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// When it was made, in milliseconds since the Unix epoch.
    pub at_ms: u64,
    pub ops: Vec<Op>,
}

/// One change an operation made to a task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Revision {
    pub at_ms: u64,
    pub task: String,
    /// The task's title after it.
    pub title: String,
    /// Its status before; none when it added the task.
    pub from: Option<Status>,
    pub to: Status,
    /// The description and note it set, if it set them.
    pub description: Option<String>,
    pub note: Option<String>,
    /// The task's weight after it.
    pub weight: f64,
    /// The task's completed share before and after it.
    pub before: f64,
    pub after: f64,
}

/// How a piece of work splits by the status of the tasks doing it, the
/// parts summing to the whole.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Shares {
    pub pending: f64,
    pub in_progress: f64,
    pub completed: f64,
    pub blocked: f64,
}

impl Shares {
    /// The part of `status`.
    pub fn of(&self, status: Status) -> f64 {
        match status {
            Status::Pending => self.pending,
            Status::InProgress => self.in_progress,
            Status::Completed => self.completed,
            Status::Blocked => self.blocked,
        }
    }

    fn add(&mut self, status: Status, part: f64) {
        match status {
            Status::Pending => self.pending += part,
            Status::InProgress => self.in_progress += part,
            Status::Completed => self.completed += part,
            Status::Blocked => self.blocked += part,
        }
    }

    fn whole(status: Status) -> Shares {
        let mut shares = Shares::default();
        shares.add(status, 1.0);
        shares
    }
}

/// One tree summed up, as its card shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Tree {
    pub id: String,
    pub title: String,
    pub status: Status,
    pub shares: Shares,
    pub leaves_done: usize,
    pub leaves: usize,
    /// When its top task was added.
    pub started_ms: u64,
    /// When the change that completed it was made, while it is completed.
    pub ended_ms: Option<u64>,
}

impl Tree {
    /// How long it has taken: until it was completed, or until `now_ms`.
    pub fn elapsed_ms(&self, now_ms: u64) -> u64 {
        self.ended_ms
            .unwrap_or(now_ms)
            .saturating_sub(self.started_ms)
    }
}

/// A task below the top of its tree, as the tree lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub task: Task,
    /// How deep it is below the top: 0 for a step of the top.
    pub depth: usize,
    pub shares: Shares,
}

/// The tasks and their history after a journal.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Timeline {
    tasks: Vec<Task>,
    revisions: Vec<Revision>,
}

impl Timeline {
    /// The timeline `entries`, oldest first, leave.
    pub fn replay<'a>(entries: impl IntoIterator<Item = &'a Entry>) -> Self {
        let mut timeline = Timeline::default();
        for entry in entries {
            timeline.apply(entry);
        }
        timeline
    }

    /// Makes the call `entry`, skipping each operation that cannot apply;
    /// why each skipped one could not.
    pub fn apply(&mut self, entry: &Entry) -> Vec<String> {
        let mut refused = Vec::new();
        for op in &entry.ops {
            if let Err(why) = self.apply_op(entry.at_ms, op) {
                refused.push(format!("{}: {why}", op.id()));
            }
        }
        refused
    }

    fn apply_op(&mut self, at_ms: u64, op: &Op) -> Result<(), String> {
        let at = self.tasks.iter().position(|t| t.id == op.id());
        let (index, from, before, description, note) = match op {
            Op::Add {
                id,
                parent,
                title,
                weight,
                status,
                description,
                note,
            } => {
                if at.is_some() {
                    return Err("already exists".into());
                }
                if let Some(parent) = parent
                    && self.task(parent).is_none()
                {
                    return Err(format!("its parent {parent} does not exist"));
                }
                self.tasks.push(Task {
                    id: id.clone(),
                    parent: parent.clone(),
                    title: title.clone(),
                    status: status.unwrap_or_default(),
                    weight: sane_weight(*weight)?.unwrap_or(1.0),
                    description: description.clone(),
                    note: note.clone(),
                });
                (self.tasks.len() - 1, None, 0.0, description, note)
            }
            Op::Update {
                status,
                title,
                weight,
                description,
                note,
                ..
            } => {
                let index = at.ok_or("no such task")?;
                let weight = sane_weight(*weight)?;
                let before = self.shares_of(op.id()).completed;
                let task = &mut self.tasks[index];
                let from = task.status;
                if let Some(status) = status {
                    task.status = *status;
                }
                if let Some(title) = title {
                    task.title = title.clone();
                }
                if let Some(weight) = weight {
                    task.weight = weight;
                }
                if let Some(description) = description {
                    task.description = Some(description.clone());
                }
                if let Some(note) = note {
                    task.note = Some(note.clone());
                }
                (index, Some(from), before, description, note)
            }
            Op::Remove { id } => {
                at.ok_or("no such task")?;
                let gone = self.subtree(id);
                self.tasks.retain(|t| !gone.contains(&t.id));
                self.revisions.retain(|r| !gone.contains(&r.task));
                return Ok(());
            }
        };
        let task = &self.tasks[index];
        self.revisions.push(Revision {
            at_ms,
            task: task.id.clone(),
            title: task.title.clone(),
            from,
            to: task.status,
            description: description.clone(),
            note: note.clone(),
            weight: task.weight,
            before,
            after: self.shares_of(&task.id).completed,
        });
        Ok(())
    }

    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    pub fn task(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    /// The steps of `parent`, or the tops of the trees, in the order added.
    pub fn children<'a>(&'a self, parent: Option<&'a str>) -> impl Iterator<Item = &'a Task> {
        self.tasks
            .iter()
            .filter(move |t| t.parent.as_deref() == parent)
    }

    fn has_children(&self, id: &str) -> bool {
        self.children(Some(id)).next().is_some()
    }

    /// `id` and every task under it.
    fn subtree(&self, id: &str) -> Vec<String> {
        let mut ids = vec![id.to_owned()];
        let mut at = 0;
        while at < ids.len() {
            let more: Vec<String> = self
                .children(Some(&ids[at]))
                .map(|t| t.id.clone())
                .collect();
            ids.extend(more);
            at += 1;
        }
        ids
    }

    /// How the work of `id` splits by status: a completed task, or one
    /// without steps, counts whole as its own status, and otherwise its
    /// steps split it by weight. All zero for a task that does not exist.
    pub fn shares_of(&self, id: &str) -> Shares {
        let Some(task) = self.task(id) else {
            return Shares::default();
        };
        if task.status == Status::Completed || !self.has_children(id) {
            return Shares::whole(task.status);
        }
        let mut shares = Shares::default();
        self.split(id, 1.0, &mut shares);
        shares
    }

    fn split(&self, parent: &str, share: f64, into: &mut Shares) {
        let total: f64 = self.children(Some(parent)).map(|t| t.weight).sum();
        if total <= 0.0 {
            return;
        }
        for task in self.children(Some(parent)) {
            let part = share * task.weight / total;
            if task.status != Status::Completed && self.has_children(&task.id) {
                self.split(&task.id, part, into);
            } else {
                into.add(task.status, part);
            }
        }
    }

    fn leaves(&self, id: &str) -> Vec<&Task> {
        if !self.has_children(id) {
            return self.task(id).into_iter().collect();
        }
        self.children(Some(id))
            .flat_map(|t| self.leaves(&t.id))
            .collect()
    }

    /// Every tree summed up, in the order its top task was added.
    pub fn trees(&self) -> Vec<Tree> {
        self.children(None).map(|top| self.tree(top)).collect()
    }

    fn tree(&self, top: &Task) -> Tree {
        let leaves = self.leaves(&top.id);
        let done = leaves
            .iter()
            .filter(|t| t.status == Status::Completed)
            .count();
        let status = if top.status == Status::Completed || done == leaves.len() {
            Status::Completed
        } else if leaves.iter().any(|t| t.status == Status::Blocked) {
            Status::Blocked
        } else if done > 0 || leaves.iter().any(|t| t.status == Status::InProgress) {
            Status::InProgress
        } else {
            Status::Pending
        };
        let ids = self.subtree(&top.id);
        let of_tree = || self.revisions.iter().filter(|r| ids.contains(&r.task));
        Tree {
            id: top.id.clone(),
            title: top.title.clone(),
            status,
            shares: self.shares_of(&top.id),
            leaves_done: done,
            leaves: leaves.len(),
            started_ms: of_tree()
                .find(|r| r.task == top.id && r.from.is_none())
                .map_or(0, |r| r.at_ms),
            ended_ms: (status == Status::Completed)
                .then(|| of_tree().map(|r| r.at_ms).max())
                .flatten(),
        }
    }

    /// The tree to show first: the one a task is in progress in, else the
    /// one added last.
    pub fn current(&self) -> Option<&Task> {
        let working = self
            .tasks
            .iter()
            .rev()
            .find(|t| t.status == Status::InProgress && !self.has_children(&t.id))
            .or_else(|| self.tasks.iter().find(|t| t.status == Status::InProgress));
        match working {
            Some(task) => Some(self.top_of(task)),
            None => self.children(None).last(),
        }
    }

    fn top_of<'a>(&'a self, task: &'a Task) -> &'a Task {
        let mut at = task;
        while let Some(parent) = at.parent.as_deref().and_then(|p| self.task(p)) {
            at = parent;
        }
        at
    }

    /// The task in progress in the tree of `top`: the deepest one, the last
    /// added among those as deep.
    pub fn working(&self, top: &str) -> Option<&Task> {
        let rows = self.rows(top);
        let deepest = rows
            .iter()
            .filter(|r| r.task.status == Status::InProgress)
            .fold(None::<&Row>, |best, r| match best {
                Some(b) if b.depth > r.depth => Some(b),
                _ => Some(r),
            });
        deepest
            .and_then(|r| self.task(&r.task.id))
            .or_else(|| self.task(top).filter(|t| t.status == Status::InProgress))
    }

    /// The tasks below `top`, each before its own steps.
    pub fn rows(&self, top: &str) -> Vec<Row> {
        let mut rows = Vec::new();
        self.walk(top, 0, &mut rows);
        rows
    }

    fn walk(&self, parent: &str, depth: usize, rows: &mut Vec<Row>) {
        for task in self.children(Some(parent)) {
            rows.push(Row {
                task: task.clone(),
                depth,
                shares: self.shares_of(&task.id),
            });
            self.walk(&task.id, depth + 1, rows);
        }
    }

    /// The changes made to `id`, the newest first.
    pub fn history<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a Revision> {
        self.revisions.iter().rev().filter(move |r| r.task == id)
    }

    /// The trees as text, as the tools answer with them.
    pub fn text(&self) -> String {
        if self.tasks.is_empty() {
            return "(no tasks)".into();
        }
        let mut lines = Vec::new();
        for top in self.children(None) {
            let tree = self.tree(top);
            lines.push(format!(
                "{} {}: {} (w={}) — {}, {} completed, {}/{} leaf tasks{}",
                top.status.mark(),
                top.id,
                top.title,
                top.weight,
                tree.status.word(),
                percent(tree.shares.completed),
                tree.leaves_done,
                tree.leaves,
                top.note
                    .as_deref()
                    .map_or_else(String::new, |n| format!(" — {n}")),
            ));
            for row in self.rows(&top.id) {
                lines.push(format!(
                    "{}- {} {}: {} (w={}){}",
                    "  ".repeat(row.depth + 1),
                    row.task.status.mark(),
                    row.task.id,
                    row.task.title,
                    row.task.weight,
                    row.task
                        .note
                        .as_deref()
                        .map_or_else(String::new, |n| format!(" — {n}")),
                ));
            }
        }
        lines.join("\n")
    }
}

/// A share as a percentage with two decimals.
pub fn percent(share: f64) -> String {
    format!("{:.2}%", share * 100.0)
}

fn sane_weight(weight: Option<f64>) -> Result<Option<f64>, String> {
    match weight {
        Some(w) if !w.is_finite() || w < 0.0 => {
            Err(format!("weight {w} is not a size of zero or more"))
        }
        _ => Ok(weight),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add(id: &str, parent: Option<&str>, weight: f64) -> Op {
        Op::Add {
            id: id.into(),
            parent: parent.map(Into::into),
            title: id.to_uppercase(),
            weight: Some(weight),
            status: None,
            description: None,
            note: None,
        }
    }

    fn set(id: &str, status: Status) -> Op {
        Op::Update {
            id: id.into(),
            status: Some(status),
            title: None,
            weight: None,
            description: None,
            note: None,
        }
    }

    fn at(at_ms: u64, ops: Vec<Op>) -> Entry {
        Entry { at_ms, ops }
    }

    fn timeline(ops: Vec<Op>) -> Timeline {
        Timeline::replay(&[at(1, ops)])
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn siblings_split_their_parent_s_share_by_weight() {
        let t = timeline(vec![
            add("goal", None, 1.0),
            add("big", Some("goal"), 3.0),
            add("small", Some("goal"), 1.0),
            set("big", Status::Completed),
        ]);
        assert!(close(t.shares_of("goal").completed, 0.75));
        assert!(close(t.shares_of("goal").pending, 0.25));
    }

    #[test]
    fn splitting_a_task_into_steps_leaves_its_share_unchanged() {
        let mut t = timeline(vec![
            add("goal", None, 1.0),
            add("a", Some("goal"), 1.0),
            add("b", Some("goal"), 1.0),
            set("a", Status::Completed),
        ]);
        let before = t.shares_of("goal").completed;
        t.apply(&at(
            2,
            vec![add("b1", Some("b"), 1.0), add("b2", Some("b"), 2.0)],
        ));
        assert!(close(t.shares_of("goal").completed, before));
        t.apply(&at(3, vec![set("b1", Status::Completed)]));
        assert!(close(t.shares_of("goal").completed, 0.5 + 0.5 / 3.0));
    }

    #[test]
    fn a_completed_task_counts_whole_even_with_steps_left() {
        let t = timeline(vec![
            add("goal", None, 1.0),
            add("a", Some("goal"), 1.0),
            add("a1", Some("a"), 1.0),
            add("b", Some("goal"), 1.0),
            set("a", Status::Completed),
        ]);
        assert!(close(t.shares_of("goal").completed, 0.5));
        assert!(close(t.shares_of("a").completed, 1.0));
    }

    #[test]
    fn the_shares_of_a_piece_of_work_sum_to_the_whole() {
        let t = timeline(vec![
            add("goal", None, 1.0),
            add("a", Some("goal"), 2.0),
            add("b", Some("goal"), 1.0),
            add("c", Some("goal"), 1.0),
            set("a", Status::InProgress),
            set("b", Status::Blocked),
        ]);
        let s = t.shares_of("goal");
        assert!(close(s.in_progress, 0.5));
        assert!(close(s.blocked, 0.25));
        assert!(close(s.pending, 0.25));
        assert!(close(
            Status::ALL.iter().map(|st| s.of(*st)).sum::<f64>(),
            1.0
        ));
    }

    #[test]
    fn a_tree_stands_by_its_leaf_tasks() {
        let cases: &[(&[Status], Status)] = &[
            (&[Status::Pending, Status::Pending], Status::Pending),
            (&[Status::InProgress, Status::Pending], Status::InProgress),
            (&[Status::Completed, Status::Pending], Status::InProgress),
            (&[Status::Completed, Status::Blocked], Status::Blocked),
            (&[Status::Completed, Status::Completed], Status::Completed),
        ];
        for (leaves, want) in cases {
            let mut ops = vec![add("goal", None, 1.0)];
            for (n, status) in leaves.iter().enumerate() {
                let id = format!("t{n}");
                ops.push(add(&id, Some("goal"), 1.0));
                ops.push(set(&id, *status));
            }
            let tree = &timeline(ops).trees()[0];
            assert_eq!(tree.status, *want, "{leaves:?}");
            assert_eq!(tree.leaves, leaves.len());
        }
    }

    #[test]
    fn a_tree_whose_top_is_completed_is_completed() {
        let t = timeline(vec![
            add("goal", None, 1.0),
            add("a", Some("goal"), 1.0),
            set("goal", Status::Completed),
        ]);
        assert_eq!(t.trees()[0].status, Status::Completed);
    }

    #[test]
    fn a_tree_counts_its_leaf_tasks_and_a_lone_top_as_one() {
        let t = timeline(vec![
            add("goal", None, 1.0),
            add("a", Some("goal"), 1.0),
            add("a1", Some("a"), 1.0),
            add("a2", Some("a"), 1.0),
            add("b", Some("goal"), 1.0),
            set("a1", Status::Completed),
            add("lone", None, 1.0),
        ]);
        let trees = t.trees();
        assert_eq!((trees[0].leaves_done, trees[0].leaves), (1, 3));
        assert_eq!((trees[1].leaves_done, trees[1].leaves), (0, 1));
    }

    #[test]
    fn a_tree_s_time_runs_from_its_top_being_added_until_it_is_completed() {
        let mut t = Timeline::replay(&[
            at(
                1_000,
                vec![add("goal", None, 1.0), add("a", Some("goal"), 1.0)],
            ),
            at(5_000, vec![set("a", Status::InProgress)]),
        ]);
        let tree = &t.trees()[0];
        assert_eq!((tree.started_ms, tree.ended_ms), (1_000, None));
        assert_eq!(tree.elapsed_ms(61_000), 60_000);
        t.apply(&at(9_000, vec![set("a", Status::Completed)]));
        let tree = &t.trees()[0];
        assert_eq!(tree.ended_ms, Some(9_000));
        assert_eq!(tree.elapsed_ms(61_000), 8_000);
    }

    #[test]
    fn each_change_to_a_task_is_kept_with_its_share_before_and_after() {
        let t = Timeline::replay(&[
            at(1, vec![add("goal", None, 1.0), add("a", Some("goal"), 2.0)]),
            at(
                2,
                vec![Op::Update {
                    id: "a".into(),
                    status: Some(Status::Completed),
                    title: None,
                    weight: None,
                    description: None,
                    note: Some("done it".into()),
                }],
            ),
        ]);
        let history: Vec<_> = t.history("a").collect();
        assert_eq!(history.len(), 2);
        let newest = history[0];
        assert_eq!(newest.at_ms, 2);
        assert_eq!(
            (newest.from, newest.to),
            (Some(Status::Pending), Status::Completed)
        );
        assert_eq!(newest.note.as_deref(), Some("done it"));
        assert_eq!(newest.weight, 2.0);
        assert!(close(newest.before, 0.0) && close(newest.after, 1.0));
        assert_eq!(history[1].from, None);
        assert_eq!(t.task("a").unwrap().note.as_deref(), Some("done it"));
    }

    #[test]
    fn an_operation_that_cannot_apply_is_skipped_and_the_rest_are_made() {
        let mut t = timeline(vec![add("goal", None, 1.0)]);
        let refused = t.apply(&at(
            2,
            vec![
                add("goal", None, 1.0),
                add("orphan", Some("nowhere"), 1.0),
                set("ghost", Status::Completed),
                add("bad", Some("goal"), -1.0),
                add("a", Some("goal"), 1.0),
            ],
        ));
        assert_eq!(refused.len(), 4, "{refused:?}");
        assert!(refused[0].starts_with("goal:"));
        assert_eq!(
            t.tasks().iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["goal", "a"]
        );
    }

    #[test]
    fn removing_a_task_removes_its_steps_and_their_history() {
        let mut t = timeline(vec![
            add("goal", None, 1.0),
            add("a", Some("goal"), 1.0),
            add("a1", Some("a"), 1.0),
            add("b", Some("goal"), 1.0),
        ]);
        t.apply(&at(2, vec![Op::Remove { id: "a".into() }]));
        assert_eq!(
            t.tasks().iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["goal", "b"]
        );
        assert_eq!(t.history("a1").count(), 0);
    }

    #[test]
    fn the_tree_shown_first_is_the_one_at_work_else_the_newest() {
        let mut t = timeline(vec![
            add("one", None, 1.0),
            add("one-a", Some("one"), 1.0),
            add("two", None, 1.0),
        ]);
        assert_eq!(t.current().map(|t| t.id.as_str()), Some("two"));
        t.apply(&at(2, vec![set("one-a", Status::InProgress)]));
        assert_eq!(t.current().map(|t| t.id.as_str()), Some("one"));
        assert_eq!(Timeline::default().current(), None);
    }

    #[test]
    fn the_task_at_work_is_the_deepest_in_progress() {
        let t = timeline(vec![
            add("goal", None, 1.0),
            add("a", Some("goal"), 1.0),
            add("a1", Some("a"), 1.0),
            set("goal", Status::InProgress),
            set("a", Status::InProgress),
            set("a1", Status::InProgress),
        ]);
        assert_eq!(t.working("goal").map(|t| t.id.as_str()), Some("a1"));
        let lone = timeline(vec![
            add("goal", None, 1.0),
            set("goal", Status::InProgress),
        ]);
        assert_eq!(lone.working("goal").map(|t| t.id.as_str()), Some("goal"));
    }

    #[test]
    fn a_tree_lists_each_task_before_its_steps_with_its_depth() {
        let t = timeline(vec![
            add("goal", None, 1.0),
            add("a", Some("goal"), 1.0),
            add("b", Some("goal"), 1.0),
            add("a1", Some("a"), 1.0),
        ]);
        let rows: Vec<_> = t
            .rows("goal")
            .into_iter()
            .map(|r| (r.task.id, r.depth))
            .collect();
        assert_eq!(rows, [("a".into(), 0), ("a1".into(), 1), ("b".into(), 0)]);
    }

    #[test]
    fn an_operation_reads_as_the_tool_takes_it() {
        let op: Op = serde_json::from_value(serde_json::json!({
            "op": "add", "id": "a", "title": "A", "status": "in_progress", "weight": 2
        }))
        .unwrap();
        assert_eq!(
            op,
            Op::Add {
                id: "a".into(),
                parent: None,
                title: "A".into(),
                weight: Some(2.0),
                status: Some(Status::InProgress),
                description: None,
                note: None,
            }
        );
        assert!(
            serde_json::from_value::<Op>(
                serde_json::json!({"op": "update", "id": "a", "colour": "red"})
            )
            .is_err()
        );
    }

    #[test]
    fn the_text_lists_every_tree_with_its_progress() {
        let t = timeline(vec![
            add("goal", None, 1.0),
            add("a", Some("goal"), 1.0),
            add("b", Some("goal"), 1.0),
            set("a", Status::Completed),
        ]);
        assert_eq!(
            t.text(),
            "[ ] goal: GOAL (w=1) — in progress, 50.00% completed, 1/2 leaf tasks\n  - [x] a: A (w=1)\n  - [ ] b: B (w=1)"
        );
        assert_eq!(Timeline::default().text(), "(no tasks)");
    }
}

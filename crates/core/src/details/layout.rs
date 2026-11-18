//! What the details panel is made of: the components the user places into
//! tabs and a foot, for each thing the panel can describe.

use adroit_model::config::UsageSettings;
use serde::{Deserialize, Deserializer, Serialize};

use crate::icons::Icon;

/// One piece of what the details panel can show.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Component {
    /// A heading of the user's own over what follows it.
    Section,
    /// The session's name, state, latest model, when it started and last
    /// took input.
    Session,
    /// The change the worktree is on, which opens the files it changes.
    Change,
    /// The task trees the session's agent recorded; folded, a summary of the
    /// one at work.
    Timeline,
    /// The newest task tree of each session its scope takes in.
    Timelines,
    /// What works for the session right now: subagents at work, live work
    /// processes and sessions tied to it that still run.
    Running,
    /// The live processes the session started, each to stop.
    Processes,
    /// The facts of the session that change little.
    About,
    /// The sessions tied to the session.
    Sessions,
    /// The messages the session sent and received through Adroit.
    Messages,
    /// The subagents the session set to work.
    Subagents,
    /// The claude.ai artifacts the conversation brought up.
    Artifacts,
    /// The images the console drew and the image files the session came
    /// across.
    Images,
    /// The other web addresses the conversation brought up.
    Links,
    /// The GitHub pull requests the conversation brought up.
    Pulls,
    /// The files the session wrote, edited, read or named.
    Files,
    /// The current change's description and the files it changes.
    Changes,
    /// The changes that lead to the worktree, as a graph.
    Log,
    /// The prompts sent to the session, newest first.
    Prompts,
    /// How full the context of the shown conversation is.
    Context,
    /// The agent account's limit of the session it is in, that resets
    /// within hours.
    SessionLimit,
    /// The agent account's limit of the week.
    WeekLimit,
    /// The agent account's other limits: those of the week for one model,
    /// and any the agent names on its own.
    ModelLimits,
    /// The whole machine's CPU use.
    Cpu,
    /// The GPU's use, when the system reports it.
    Gpu,
    /// The physical memory used.
    Memory,
    /// The capacity used on the startup volume.
    Storage,
    /// How the connection to the environment stands.
    Environment,
    /// A button that tries the environment's connection again at once, while
    /// it can be tried.
    EnvironmentRetry,
    /// A button that pairs with the environment's work machine again.
    EnvironmentPair,
    /// A button that opens Settings on the environment.
    EnvironmentSettings,
    /// The changes and drafts waiting for the environment.
    EnvironmentWaiting,
    /// The environment's agent profiles, with their logins and usage.
    Profiles,
    /// The pull request or issue: the one selected in the tree, or else the
    /// pull request of the worktree's branch.
    Github,
    /// A button that shows the pull request or issue in the content.
    GithubShow,
    /// A button that opens the pull request or issue on GitHub.
    GithubOpen,
    /// A button that starts a session on the pull request or issue.
    GithubStart,
    /// What was said on the pull request or issue, each post in brief.
    GithubConversation,
    /// The files the pull request changes, each with its lines added and
    /// taken out.
    GithubFiles,
    /// The checks on the pull request's head, those that failed first.
    GithubChecks,
}

/// What the details panel describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Subject {
    /// What the content's focused window shows: a session, a draft, a shell
    /// or a file, in a worktree.
    Session,
    /// An environment's heading selected in the tree.
    Environment,
    /// A pull request's or an issue's row selected in the tree.
    Github,
}

/// What a component is of, as the Settings window groups them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Layout,
    Session,
    Resources,
    Repository,
    Agent,
    Machine,
}

impl Kind {
    pub const ALL: &[Kind] = &[
        Kind::Layout,
        Kind::Session,
        Kind::Resources,
        Kind::Repository,
        Kind::Agent,
        Kind::Machine,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Kind::Layout => "Layout",
            Kind::Session => "Session",
            Kind::Resources => "Resources",
            Kind::Repository => "Repository",
            Kind::Agent => "Agent usage",
            Kind::Machine => "Machine",
        }
    }
}

impl Component {
    pub const ALL: &[Component] = &[
        Component::Section,
        Component::Session,
        Component::Change,
        Component::Timeline,
        Component::Timelines,
        Component::Running,
        Component::Processes,
        Component::About,
        Component::Sessions,
        Component::Messages,
        Component::Subagents,
        Component::Artifacts,
        Component::Images,
        Component::Links,
        Component::Pulls,
        Component::Files,
        Component::Changes,
        Component::Log,
        Component::Prompts,
        Component::Context,
        Component::SessionLimit,
        Component::WeekLimit,
        Component::ModelLimits,
        Component::Cpu,
        Component::Gpu,
        Component::Memory,
        Component::Storage,
        Component::Environment,
        Component::EnvironmentRetry,
        Component::EnvironmentPair,
        Component::EnvironmentSettings,
        Component::EnvironmentWaiting,
        Component::Profiles,
        Component::Github,
        Component::GithubShow,
        Component::GithubOpen,
        Component::GithubStart,
        Component::GithubConversation,
        Component::GithubFiles,
        Component::GithubChecks,
    ];

    /// Its name, as the settings write it.
    pub fn name(self) -> &'static str {
        match self {
            Component::Section => "section",
            Component::Session => "session",
            Component::Change => "change",
            Component::Timeline => "timeline",
            Component::Timelines => "timelines",
            Component::Running => "running",
            Component::Processes => "processes",
            Component::About => "about",
            Component::Sessions => "sessions",
            Component::Messages => "messages",
            Component::Subagents => "subagents",
            Component::Artifacts => "artifacts",
            Component::Images => "images",
            Component::Links => "links",
            Component::Pulls => "pulls",
            Component::Files => "files",
            Component::Changes => "changes",
            Component::Log => "log",
            Component::Prompts => "prompts",
            Component::Context => "context",
            Component::SessionLimit => "session-limit",
            Component::WeekLimit => "week-limit",
            Component::ModelLimits => "model-limits",
            Component::Cpu => "cpu",
            Component::Gpu => "gpu",
            Component::Memory => "memory",
            Component::Storage => "storage",
            Component::Environment => "environment",
            Component::EnvironmentRetry => "environment-retry",
            Component::EnvironmentPair => "environment-pair",
            Component::EnvironmentSettings => "environment-settings",
            Component::EnvironmentWaiting => "environment-waiting",
            Component::Profiles => "profiles",
            Component::Github => "github",
            Component::GithubShow => "github-show",
            Component::GithubOpen => "github-open",
            Component::GithubStart => "github-start",
            Component::GithubConversation => "github-conversation",
            Component::GithubFiles => "github-files",
            Component::GithubChecks => "github-checks",
        }
    }

    /// Its name as the Settings window shows it.
    pub fn title(self) -> &'static str {
        match self {
            Component::Section => "Section heading",
            Component::Session => "Session",
            Component::Change => "Current change",
            Component::Timeline => "Timeline",
            Component::Timelines => "Session timelines",
            Component::Running => "Running",
            Component::Processes => "Processes",
            Component::About => "About",
            Component::Sessions => "Sessions",
            Component::Messages => "Messages",
            Component::Subagents => "Subagents",
            Component::Artifacts => "Artifacts",
            Component::Images => "Images",
            Component::Links => "Links",
            Component::Pulls => "Pull requests",
            Component::Files => "Files",
            Component::Changes => "Changes",
            Component::Log => "Log",
            Component::Prompts => "Prompts",
            Component::Context => "Context",
            Component::SessionLimit => "Session limit",
            Component::WeekLimit => "Weekly limit",
            Component::ModelLimits => "Per-model limits",
            Component::Cpu => "CPU",
            Component::Gpu => "GPU",
            Component::Memory => "Memory",
            Component::Storage => "Storage",
            Component::Environment => "Environment",
            Component::EnvironmentRetry => "Retry now button",
            Component::EnvironmentPair => "Pair again button",
            Component::EnvironmentSettings => "Environment settings button",
            Component::EnvironmentWaiting => "Waiting changes",
            Component::Profiles => "Profiles",
            Component::Github => "GitHub",
            Component::GithubShow => "Open button",
            Component::GithubOpen => "GitHub button",
            Component::GithubStart => "Start session button",
            Component::GithubConversation => "PR conversation",
            Component::GithubFiles => "PR files",
            Component::GithubChecks => "PR checks",
        }
    }

    /// What it shows, in a line, as the Settings window tells it.
    pub fn about(self) -> &'static str {
        match self {
            Component::Section => "A heading of your own, such as Usage, over what follows it.",
            Component::Session => {
                "The session's name, state and model, when it started and last took input."
            }
            Component::Change => {
                "The change the worktree is on; it opens the files the change touches."
            }
            Component::Timeline => {
                "The task trees the session's agent recorded, the newest first, each folding on its own."
            }
            Component::Timelines => {
                "The newest task tree of each session its scope takes in, side by side."
            }
            Component::Running => {
                "Subagents at work, live processes and tied sessions still running; folded, how many of each."
            }
            Component::Processes => "The live processes the session started, each to stop.",
            Component::About => "The facts of the session that change little.",
            Component::Sessions => "The sessions tied to this one.",
            Component::Messages => "What the session sent and received through Adroit.",
            Component::Subagents => "The subagents the session set to work.",
            Component::Artifacts => "The claude.ai artifacts the conversation brought up.",
            Component::Images => "The images the console drew and the image files seen.",
            Component::Links => "The other web addresses the conversation brought up.",
            Component::Pulls => {
                "The pull requests the conversation brought up, as GitHub last told of them."
            }
            Component::Files => "The files the session wrote, edited, read or named.",
            Component::Changes => "The current change's description and its files.",
            Component::Log => "The changes that lead to the worktree, as a graph.",
            Component::Prompts => "The prompts sent to the session, newest first.",
            Component::Context => "How full the conversation's context is.",
            Component::SessionLimit => "The account's limit that resets within hours.",
            Component::WeekLimit => "The account's limit of the week.",
            Component::ModelLimits => "The account's weekly limits of one model, and any other.",
            Component::Cpu => "The whole machine's CPU use.",
            Component::Gpu => "The GPU's use, when the system reports it.",
            Component::Memory => "The physical memory used.",
            Component::Storage => "The capacity used on the startup volume.",
            Component::Environment => "How the connection to the environment stands.",
            Component::EnvironmentRetry => {
                "A button that tries the connection again, while it can be."
            }
            Component::EnvironmentPair => "A button that pairs with the work machine again.",
            Component::EnvironmentSettings => "A button that opens Settings on the environment.",
            Component::EnvironmentWaiting => "The changes and drafts waiting for the environment.",
            Component::Profiles => "The environment's agent profiles, their logins and usage.",
            Component::Github => {
                "The pull request or issue selected, or the branch's pull request."
            }
            Component::GithubShow => "A button that opens the pull request or issue in Adroit.",
            Component::GithubOpen => "A button that opens the pull request or issue on GitHub.",
            Component::GithubStart => {
                "A button that starts a session on the pull request or issue."
            }
            Component::GithubConversation => {
                "What was said on the pull request or issue, each post in brief."
            }
            Component::GithubFiles => {
                "The files the pull request changes, with their lines added and taken out."
            }
            Component::GithubChecks => "The checks on the pull request, those that failed first.",
        }
    }

    /// The icon it is shown with in the Settings window.
    pub fn icon(self) -> Icon {
        match self {
            Component::Section => Icon::CursorText,
            Component::Session => Icon::Terminal2,
            Component::Change => Icon::GitBranch,
            Component::Timeline => Icon::TimelineEvent,
            Component::Timelines => Icon::Timeline,
            Component::Running => Icon::Loader,
            Component::Processes => Icon::Terminal,
            Component::About => Icon::InfoCircle,
            Component::Sessions => Icon::UsersGroup,
            Component::Messages => Icon::Message,
            Component::Subagents => Icon::ListTree,
            Component::Artifacts => Icon::AppWindow,
            Component::Images => Icon::Photo,
            Component::Links => Icon::Link,
            Component::Pulls => Icon::GitPullRequest,
            Component::Files => Icon::File,
            Component::Changes => Icon::FileDiff,
            Component::Log => Icon::GitCommit,
            Component::Prompts => Icon::Send,
            Component::Context => Icon::Notes,
            Component::SessionLimit => Icon::RotateClockwise,
            Component::WeekLimit => Icon::Refresh,
            Component::ModelLimits => Icon::AdjustmentsHorizontal,
            Component::Cpu => Icon::Cpu,
            Component::Gpu => Icon::Gpu,
            Component::Memory => Icon::Memory,
            Component::Storage => Icon::Database,
            Component::Environment => Icon::ServerBolt,
            Component::EnvironmentRetry => Icon::ArrowBackUp,
            Component::EnvironmentPair => Icon::ArrowsExchange,
            Component::EnvironmentSettings => Icon::Settings,
            Component::EnvironmentWaiting => Icon::Bell,
            Component::Profiles => Icon::Lock,
            Component::Github => Icon::BrandGithub,
            Component::GithubShow => Icon::ArrowRight,
            Component::GithubOpen => Icon::SquareArrowRight,
            Component::GithubStart => Icon::MessagePlus,
            Component::GithubConversation => Icon::MessageDown,
            Component::GithubFiles => Icon::FilePlus,
            Component::GithubChecks => Icon::Check,
        }
    }

    /// What it is of, as the Settings window groups the components.
    pub fn kind(self) -> Kind {
        match self {
            Component::Section => Kind::Layout,
            Component::Session
            | Component::Change
            | Component::Timeline
            | Component::Timelines
            | Component::Running
            | Component::Processes
            | Component::About
            | Component::Sessions
            | Component::Messages
            | Component::Subagents
            | Component::Prompts => Kind::Session,
            Component::Artifacts
            | Component::Images
            | Component::Links
            | Component::Pulls
            | Component::Files => Kind::Resources,
            Component::Changes
            | Component::Log
            | Component::Github
            | Component::GithubShow
            | Component::GithubOpen
            | Component::GithubStart
            | Component::GithubConversation
            | Component::GithubFiles
            | Component::GithubChecks => Kind::Repository,
            Component::Context
            | Component::SessionLimit
            | Component::WeekLimit
            | Component::ModelLimits => Kind::Agent,
            Component::Cpu
            | Component::Gpu
            | Component::Memory
            | Component::Storage
            | Component::Environment
            | Component::EnvironmentRetry
            | Component::EnvironmentPair
            | Component::EnvironmentSettings
            | Component::EnvironmentWaiting
            | Component::Profiles => Kind::Machine,
        }
    }

    /// The machine's meter it is, with nothing else shown.
    pub fn meter(self) -> Option<UsageSettings> {
        let none = UsageSettings::NONE;
        match self {
            Component::Cpu => Some(UsageSettings { cpu: true, ..none }),
            Component::Gpu => Some(UsageSettings { gpu: true, ..none }),
            Component::Memory => Some(UsageSettings {
                memory: true,
                ..none
            }),
            Component::Storage => Some(UsageSettings {
                storage: true,
                ..none
            }),
            _ => None,
        }
    }

    /// The kind of the agent's gauge it is.
    pub fn gauge(self) -> Option<crate::details::GaugeKind> {
        use crate::details::GaugeKind;
        match self {
            Component::Context => Some(GaugeKind::Context),
            Component::SessionLimit => Some(GaugeKind::Session),
            Component::WeekLimit => Some(GaugeKind::Week),
            Component::ModelLimits => Some(GaugeKind::Other),
            _ => None,
        }
    }

    /// Whether it can describe `subject`. What is of the environment can
    /// describe all of them; the rest only what a window shows, save the
    /// pull request or issue, which a row of the tree also is.
    pub fn describes(self, subject: Subject) -> bool {
        match self {
            Component::Environment
            | Component::EnvironmentRetry
            | Component::EnvironmentPair
            | Component::EnvironmentSettings
            | Component::EnvironmentWaiting
            | Component::Profiles
            | Component::Section => true,
            c if c.meter().is_some() => true,
            Component::Github
            | Component::GithubShow
            | Component::GithubOpen
            | Component::GithubStart
            | Component::GithubConversation
            | Component::GithubFiles
            | Component::GithubChecks => matches!(subject, Subject::Session | Subject::Github),
            _ => subject == Subject::Session,
        }
    }

    /// Every component that can describe `subject`, as the Settings window
    /// offers them.
    pub fn placeable(subject: Subject) -> impl Iterator<Item = Component> {
        Component::ALL
            .iter()
            .copied()
            .filter(move |c| c.describes(subject))
    }

    /// Whether it tells of a pull request or issue, the branch's where a
    /// window is described.
    pub fn is_github(self) -> bool {
        matches!(
            self,
            Component::Github
                | Component::GithubShow
                | Component::GithubOpen
                | Component::GithubStart
                | Component::GithubConversation
                | Component::GithubFiles
                | Component::GithubChecks
        )
    }

    /// Whether it is a button, which can be given a tone.
    pub fn is_button(self) -> bool {
        matches!(
            self,
            Component::GithubShow
                | Component::GithubOpen
                | Component::GithubStart
                | Component::EnvironmentRetry
                | Component::EnvironmentPair
                | Component::EnvironmentSettings
        )
    }

    /// The label a button has unless its placement says.
    pub fn label(self) -> &'static str {
        match self {
            Component::GithubShow => "Open",
            Component::GithubOpen => "GitHub",
            Component::GithubStart => "Start a session",
            Component::EnvironmentRetry => "Retry now",
            Component::EnvironmentPair => "Pair again",
            Component::EnvironmentSettings => "Environment settings",
            c => c.title(),
        }
    }

    /// The icon a button has unless its placement says.
    pub fn button_icon(self) -> Option<Icon> {
        match self {
            Component::GithubShow => Some(Icon::ArrowRight),
            Component::GithubOpen => Some(Icon::BrandGithub),
            Component::GithubStart => Some(Icon::MessagePlus),
            Component::EnvironmentRetry => Some(Icon::Refresh),
            Component::EnvironmentPair => Some(Icon::ArrowsExchange),
            Component::EnvironmentSettings => Some(Icon::Settings),
            _ => None,
        }
    }

    /// The tone a button has unless its placement says.
    pub fn tone(self) -> Tone {
        match self {
            Component::GithubShow | Component::EnvironmentRetry => Tone::Primary,
            _ => Tone::Plain,
        }
    }

    /// Whether it lists its first few rows and one that shows the rest, as
    /// many as its `shown` says; the timeline, its newest trees.
    pub fn pages(self) -> bool {
        (self.folds() && self != Component::About) || self == Component::Timeline
    }

    /// Whether it folds its rows under their heading.
    pub fn folds(self) -> bool {
        matches!(
            self,
            Component::Timelines
                | Component::Running
                | Component::Processes
                | Component::About
                | Component::Sessions
                | Component::Messages
                | Component::Subagents
                | Component::Artifacts
                | Component::Images
                | Component::Links
                | Component::Pulls
                | Component::Files
                | Component::Prompts
                | Component::Profiles
        )
    }
}

/// A component where it is placed, with how it shows there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    pub component: Component,
    /// What a section heading says, or a button's label; as Adroit has it
    /// when unset.
    pub title: Option<String>,
    /// Whether its heading starts folded; as Adroit has it when unset.
    pub folded: Option<bool>,
    /// The rows it shows before a row that shows them all; the details
    /// settings' when unset.
    pub shown: Option<usize>,
    /// Whether it stands beside the one before it, on the same line, rather
    /// than under it.
    pub beside: bool,
    /// How a button is coloured; as Adroit has it when unset.
    pub tone: Option<Tone>,
    /// The icon of a button, by its name in Tabler; as Adroit has it when
    /// unset, and none when empty.
    pub icon: Option<String>,
    /// Whether a button shows its icon alone, its label only as its tip.
    pub icon_only: bool,
    /// The sessions the session timelines take in; as Adroit has it when
    /// unset.
    pub scope: Option<Scope>,
}

/// Which sessions the session timelines take in, beside the one shown.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    /// The one that opened it, those it opened, and those it messaged or
    /// heard from.
    #[default]
    Related,
    /// Every session in its workspace.
    Workspace,
    /// Every session of its environment.
    Environment,
}

impl Scope {
    pub const ALL: [Scope; 3] = [Scope::Related, Scope::Workspace, Scope::Environment];

    pub fn name(self) -> &'static str {
        match self {
            Scope::Related => "related",
            Scope::Workspace => "workspace",
            Scope::Environment => "environment",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Scope::Related => "Related sessions",
            Scope::Workspace => "Its workspace",
            Scope::Environment => "Its environment",
        }
    }
}

/// How a button is coloured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Tone {
    Plain,
    Primary,
    Danger,
}

impl Tone {
    pub const ALL: [Tone; 3] = [Tone::Plain, Tone::Primary, Tone::Danger];

    pub fn name(self) -> &'static str {
        match self {
            Tone::Plain => "plain",
            Tone::Primary => "primary",
            Tone::Danger => "danger",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Tone::Plain => "Plain",
            Tone::Primary => "Primary",
            Tone::Danger => "Danger",
        }
    }
}

impl Placed {
    /// The label it shows, of a button.
    pub fn label(&self) -> String {
        self.title
            .clone()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| self.component.label().to_string())
    }

    /// The icon it shows, of a button.
    pub fn button_icon(&self) -> Option<Icon> {
        match &self.icon {
            Some(name) => Icon::ALL.iter().copied().find(|i| i.name() == name),
            None => self.component.button_icon(),
        }
    }

    /// How it is coloured, of a button.
    pub fn tone(&self) -> Tone {
        self.tone.unwrap_or(self.component.tone())
    }

    /// The sessions it takes in, of the session timelines.
    pub fn scope(&self) -> Scope {
        self.scope.unwrap_or_default()
    }

    pub const fn new(component: Component) -> Self {
        Self {
            component,
            title: None,
            folded: None,
            shown: None,
            beside: false,
            tone: None,
            icon: None,
            icon_only: false,
            scope: None,
        }
    }

    const fn folded(mut self) -> Self {
        self.folded = Some(true);
        self
    }

    const fn shown(mut self, rows: usize) -> Self {
        self.shown = Some(rows);
        self
    }

    const fn beside(mut self) -> Self {
        self.beside = true;
        self
    }

    /// `component` as it is first placed: a section heading says Section.
    pub fn first(component: Component) -> Self {
        match component {
            Component::Section => Placed::new(component).titled("Section"),
            _ => Placed::new(component),
        }
    }

    fn titled(mut self, title: &str) -> Self {
        self.title = Some(title.into());
        self
    }

    fn is_bare(&self) -> bool {
        self.title.is_none()
            && self.folded.is_none()
            && self.shown.is_none()
            && !self.beside
            && self.tone.is_none()
            && self.icon.is_none()
            && !self.icon_only
            && self.scope.is_none()
    }
}

/// A component as the settings write it: its name alone, or a table with its
/// name and how it shows.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum PlacedFile {
    /// The component's name.
    Bare(Component),
    Set {
        component: Component,
        /// What a section heading says.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Whether its heading starts folded.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        folded: Option<bool>,
        /// The rows it shows before a row that shows them all.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shown: Option<usize>,
        /// Whether it stands beside the one before it.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        beside: bool,
        /// How a button is coloured.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tone: Option<Tone>,
        /// The icon of a button, by its name in Tabler; none when empty.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
        /// Whether a button shows its icon alone.
        #[serde(
            default,
            rename = "icon-only",
            skip_serializing_if = "std::ops::Not::not"
        )]
        icon_only: bool,
        /// The sessions the session timelines take in.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<Scope>,
    },
}

// The settings reference types a component by the name it is mostly
// written as; the table it can also be written as is told in the spec.
impl schemars::JsonSchema for Placed {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Placed".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "string" })
    }
}

impl Serialize for Placed {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if self.is_bare() {
            PlacedFile::Bare(self.component).serialize(s)
        } else {
            PlacedFile::Set {
                component: self.component,
                title: self.title.clone(),
                folded: self.folded,
                shown: self.shown,
                beside: self.beside,
                tone: self.tone,
                icon: self.icon.clone(),
                icon_only: self.icon_only,
                scope: self.scope,
            }
            .serialize(s)
        }
    }
}

impl<'de> Deserialize<'de> for Placed {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match PlacedFile::deserialize(d)? {
            PlacedFile::Bare(component) => Placed::new(component),
            PlacedFile::Set {
                component,
                title,
                folded,
                shown,
                beside,
                tone,
                icon,
                icon_only,
                scope,
            } => Placed {
                component,
                title,
                folded,
                shown,
                beside,
                tone,
                icon,
                icon_only,
                scope,
            },
        })
    }
}

/// A tab of the details panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DetailsTab {
    /// Its name on the tab.
    pub title: String,
    /// The Tabler icon beside its name, by its name in Tabler; none when
    /// unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// What it shows, top first: each a component's name, or a table naming
    /// it with how it shows.
    pub components: Vec<Placed>,
}

impl DetailsTab {
    fn new(title: &str, icon: Icon, components: &[Placed]) -> Self {
        Self {
            title: title.into(),
            icon: Some(icon.name().into()),
            components: components.to_vec(),
        }
    }

    /// Its icon, when the set has one of the name it gives.
    pub fn icon(&self) -> Option<Icon> {
        let name = self.icon.as_deref()?;
        Icon::ALL.iter().copied().find(|i| i.name() == name)
    }
}

/// What the details panel shows of one subject: its tabs, and a foot shown
/// under every tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Surface {
    /// Shown under every tab, top first: each a component's name, or a table
    /// naming it with how it shows.
    #[serde(default)]
    pub foot: Vec<Placed>,
    /// The tabs, in order; the keys that show a tab name it by its place.
    pub tabs: Vec<DetailsTab>,
}

impl Surface {
    /// What a session, draft, shell or file shows when the settings do not
    /// say.
    pub fn session() -> Self {
        use Component as C;
        let p = Placed::new;
        Self {
            // Kept to two lines so the tab above keeps the room: how full the
            // conversation is, the account's limits, and the machine's load.
            foot: vec![
                p(C::Context),
                p(C::SessionLimit),
                p(C::WeekLimit).beside(),
                p(C::Cpu),
                p(C::Memory).beside(),
            ],
            tabs: vec![
                DetailsTab::new(
                    "Overview",
                    Icon::LayoutDashboard,
                    &[
                        p(C::Session),
                        p(C::Change),
                        p(C::Github),
                        p(C::Pulls),
                        p(C::Timeline).shown(3),
                        p(C::Timelines),
                        p(C::Running).folded(),
                        p(C::About).folded(),
                    ],
                ),
                DetailsTab::new("Code", Icon::GitBranch, &[p(C::Changes), p(C::Log)]),
                DetailsTab::new("Timeline", Icon::TimelineEvent, &[p(C::Timeline)]),
                DetailsTab::new(
                    "Conversation",
                    Icon::Message,
                    &[
                        p(C::Prompts),
                        p(C::Messages),
                        p(C::Subagents),
                        p(C::Sessions),
                    ],
                ),
                DetailsTab::new(
                    "Resources",
                    Icon::Link,
                    &[p(C::Files), p(C::Links), p(C::Artifacts), p(C::Images)],
                ),
            ],
        }
    }

    /// What an environment's heading shows when the settings do not say.
    pub fn environment() -> Self {
        use Component as C;
        let p = Placed::new;
        Self {
            // How the environment stands and what can be done about it, its
            // profiles in a tab of their own; its machine's meters stay in
            // sight under every tab.
            foot: vec![
                p(C::Section).titled("Machine"),
                p(C::Cpu),
                p(C::Gpu).beside(),
                p(C::Memory),
                p(C::Storage).beside(),
            ],
            tabs: vec![
                DetailsTab::new(
                    "Overview",
                    Icon::Server,
                    &[
                        p(C::Environment),
                        p(C::EnvironmentRetry),
                        p(C::EnvironmentPair).beside(),
                        p(C::EnvironmentSettings),
                        p(C::EnvironmentWaiting),
                    ],
                ),
                DetailsTab::new("Profiles", Icon::UsersGroup, &[p(C::Profiles)]),
            ],
        }
    }

    /// What a pull request's or an issue's row shows when the settings do
    /// not say.
    pub fn github() -> Self {
        Self {
            // What a pull request is at a glance and what can be done with
            // it, what was said and what it changes in tabs of their own; its
            // checks stay in sight under every tab.
            foot: vec![
                Placed::new(Component::Section).titled("PR checks"),
                Placed::new(Component::GithubChecks),
            ],
            tabs: vec![
                DetailsTab::new(
                    "Overview",
                    Icon::BrandGithub,
                    &[
                        Placed::new(Component::Github),
                        Placed::new(Component::GithubShow),
                        Placed::new(Component::GithubOpen).beside(),
                        Placed::new(Component::GithubStart),
                    ],
                ),
                DetailsTab::new(
                    "Conversation",
                    Icon::MessageDown,
                    &[Placed::new(Component::GithubConversation)],
                ),
                DetailsTab::new(
                    "Files",
                    Icon::FileDiff,
                    &[Placed::new(Component::GithubFiles)],
                ),
            ],
        }
    }

    /// Every component placed, the foot's included.
    pub fn placed(&self) -> impl Iterator<Item = &Placed> {
        self.tabs
            .iter()
            .flat_map(|t| t.components.iter())
            .chain(self.foot.iter())
    }

    /// What `slot` places: a tab's, by its place, or the foot's.
    pub fn components(&self, slot: Slot) -> Option<&[Placed]> {
        match slot {
            Slot::Tab(at) => self.tabs.get(at).map(|t| t.components.as_slice()),
            Slot::Foot => Some(&self.foot),
        }
    }

    /// The components of `slot`: a tab's, by its place, or the foot's.
    pub fn components_mut(&mut self, slot: Slot) -> Option<&mut Vec<Placed>> {
        match slot {
            Slot::Tab(at) => self.tabs.get_mut(at).map(|t| &mut t.components),
            Slot::Foot => Some(&mut self.foot),
        }
    }

    /// A tab named `title`, holding nothing yet, added after the others.
    pub fn add_tab(&mut self, title: &str) {
        self.tabs.push(DetailsTab {
            title: title.into(),
            icon: None,
            components: Vec::new(),
        });
    }

    /// The tab at `at` taken out, unless it is the only one.
    pub fn remove_tab(&mut self, at: usize) {
        if self.tabs.len() > 1 && at < self.tabs.len() {
            self.tabs.remove(at);
        }
    }

    /// The tab at `at` moved `delta` places along, stopping at either end.
    pub fn move_tab(&mut self, at: usize, delta: isize) {
        shift(&mut self.tabs, at, delta);
    }

    /// `component` placed last in `slot`.
    pub fn add(&mut self, slot: Slot, component: Component) {
        if let Some(list) = self.components_mut(slot) {
            list.push(Placed::first(component));
        }
    }

    /// `component` placed in `slot` before what is at `at`, or last when `at`
    /// is past the end.
    pub fn insert(&mut self, slot: Slot, at: usize, component: Component) {
        if let Some(list) = self.components_mut(slot) {
            list.insert(at.min(list.len()), Placed::first(component));
        }
    }

    /// What `from` places at `at` carried, as it shows, to `to` before what
    /// is at `to_at` there, counted before it was taken out.
    pub fn carry(&mut self, from: Slot, at: usize, to: Slot, to_at: usize) {
        let Some(placed) = self.components_mut(from).and_then(|l| take(l, at)) else {
            return;
        };
        let placed = Placed {
            beside: false,
            ..placed
        };
        let to_at = if from == to && to_at > at {
            to_at - 1
        } else {
            to_at
        };
        match self.components_mut(to) {
            Some(list) => list.insert(to_at.min(list.len()), placed),
            None => {
                if let Some(l) = self.components_mut(from) {
                    l.insert(at, placed);
                }
            }
        }
    }

    /// What `slot` places at `at` taken out.
    pub fn remove(&mut self, slot: Slot, at: usize) {
        if let Some(list) = self.components_mut(slot) {
            take(list, at);
        }
    }

    /// What `slot` places at `at` put beside the one before it, or on a line
    /// of its own.
    pub fn set_beside(&mut self, slot: Slot, at: usize, beside: bool) {
        if let Some(p) = self.components_mut(slot).and_then(|l| l.get_mut(at)) {
            p.beside = beside && at > 0;
        }
    }

    /// What `slot` places at `at` moved `delta` places along, stopping at
    /// either end.
    pub fn shift(&mut self, slot: Slot, at: usize, delta: isize) {
        if let Some(list) = self.components_mut(slot) {
            shift(list, at, delta);
        }
    }

    /// Why it cannot describe `subject`, if it cannot.
    fn check(&self, subject: Subject, table: &str) -> Result<(), String> {
        if self.tabs.is_empty() {
            return Err(format!("`{table}` has no tab"));
        }
        for tab in &self.tabs {
            if tab.icon.is_some() && tab.icon().is_none() {
                return Err(format!(
                    "`{table}`: the tab `{}` names an icon Adroit does not have: `{}`",
                    tab.title,
                    tab.icon.as_deref().unwrap_or_default()
                ));
            }
        }
        if let Some(p) = self.placed().find(|p| !p.component.describes(subject)) {
            return Err(format!(
                "`{table}`: `{}` cannot be placed there",
                p.component.name()
            ));
        }
        Ok(())
    }
}

/// Where a component is placed: in a tab, by its place, or in the foot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Tab(usize),
    Foot,
}

/// What `list` places at `at` taken out; one that stood beside it then
/// starts the line it started.
fn take(list: &mut Vec<Placed>, at: usize) -> Option<Placed> {
    if at >= list.len() {
        return None;
    }
    let placed = list.remove(at);
    if !placed.beside
        && let Some(next) = list.get_mut(at)
    {
        next.beside = false;
    }
    Some(placed)
}

/// The item at `at` of `list` moved `delta` places along, stopping at either
/// end.
fn shift<T>(list: &mut [T], at: usize, delta: isize) {
    if at >= list.len() {
        return;
    }
    let to = at.saturating_add_signed(delta).min(list.len() - 1);
    if to < at {
        list[to..=at].rotate_right(1);
    } else {
        list[at..=to].rotate_left(1);
    }
}

/// What the details panel is made of, for each thing it describes. Each
/// left unset is as Adroit has it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct DetailsLayout {
    /// What a session, draft, shell or file shows.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "session_surface"
    )]
    pub session: Option<Surface>,
    /// What an environment's heading shows.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "environment_surface"
    )]
    pub environment: Option<Surface>,
    /// What a pull request's or an issue's row shows.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "github_surface"
    )]
    pub github: Option<Surface>,
}

impl DetailsLayout {
    /// What `subject` shows.
    pub fn surface(&self, subject: Subject) -> Surface {
        let (set, shipped): (_, fn() -> Surface) = match subject {
            Subject::Session => (&self.session, Surface::session),
            Subject::Environment => (&self.environment, Surface::environment),
            Subject::Github => (&self.github, Surface::github),
        };
        set.clone().unwrap_or_else(shipped)
    }

    /// `subject`'s surface set to `surface`; Adroit's own leaves it unset.
    pub fn set(&mut self, subject: Subject, surface: Surface) {
        let shipped = DetailsLayout::default().surface(subject);
        let set = (surface != shipped).then_some(surface);
        match subject {
            Subject::Session => self.session = set,
            Subject::Environment => self.environment = set,
            Subject::Github => self.github = set,
        }
    }
}

fn checked<'de, D: Deserializer<'de>>(
    d: D,
    subject: Subject,
    table: &str,
) -> Result<Option<Surface>, D::Error> {
    let surface = Surface::deserialize(d)?;
    surface
        .check(subject, table)
        .map_err(serde::de::Error::custom)?;
    Ok(Some(surface))
}

fn session_surface<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Surface>, D::Error> {
    checked(d, Subject::Session, "panel.session")
}

fn environment_surface<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Surface>, D::Error> {
    checked(d, Subject::Environment, "panel.environment")
}

fn github_surface<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Surface>, D::Error> {
    checked(d, Subject::Github, "panel.github")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<DetailsLayout, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    #[test]
    fn a_layout_left_unset_is_the_one_adroit_ships() {
        let layout = parse("").unwrap();
        assert_eq!(layout.surface(Subject::Session), Surface::session());
        assert_eq!(layout.surface(Subject::Environment), Surface::environment());
        assert_eq!(layout.surface(Subject::Github), Surface::github());
    }

    #[test]
    fn the_shipped_window_panel_reads_from_a_glance_to_the_code_the_work_and_what_it_used() {
        let titles: Vec<String> = Surface::session()
            .tabs
            .into_iter()
            .map(|t| t.title)
            .collect();
        assert_eq!(
            titles,
            ["Overview", "Code", "Timeline", "Conversation", "Resources"]
        );
    }

    #[test]
    fn a_component_is_written_by_its_name_alone_or_with_how_it_shows() {
        let layout = parse(
            r#"
[session]
foot = ["cpu"]
[[session.tabs]]
title = "Mine"
components = ["files", { component = "about", folded = false, shown = 3 }]
"#,
        )
        .unwrap();
        let s = layout.session.unwrap();
        assert_eq!(s.foot, [Placed::new(Component::Cpu)]);
        let c = &s.tabs[0].components;
        assert_eq!(c[0], Placed::new(Component::Files));
        assert_eq!((c[1].folded, c[1].shown), (Some(false), Some(3)));
    }

    #[test]
    fn a_layout_reads_back_as_it_was_written() {
        let mut layout = DetailsLayout::default();
        let mut s = Surface::session();
        s.tabs[0].components.push(Placed {
            shown: Some(9),
            ..Placed::new(Component::Prompts)
        });
        layout.set(Subject::Session, s);
        let text = toml::to_string(&layout).unwrap();
        assert_eq!(parse(&text).unwrap(), layout);
    }

    #[test]
    fn every_component_is_named_as_serde_names_it() {
        for c in Component::ALL {
            let written = toml::Value::try_from(c).unwrap();
            assert_eq!(written.as_str(), Some(c.name()));
        }
    }

    #[test]
    fn a_surface_set_to_the_one_adroit_ships_is_left_unset() {
        let mut layout = DetailsLayout::default();
        layout.set(Subject::Environment, Surface::environment());
        assert_eq!(layout, DetailsLayout::default());
    }

    #[test]
    fn a_component_that_cannot_describe_the_subject_is_refused() {
        let err = parse(
            r#"
[[environment.tabs]]
title = "Env"
components = ["timeline"]
"#,
        )
        .unwrap_err();
        assert!(err.contains("`timeline` cannot be placed there"), "{err}");
        assert!(
            parse(
                "[github]\nfoot = [\"changes\"]\n[[github.tabs]]\ntitle = \"G\"\ncomponents = []\n"
            )
            .is_err()
        );
    }

    #[test]
    fn an_unknown_component_an_unknown_icon_or_no_tab_is_refused() {
        assert!(parse("[[session.tabs]]\ntitle = \"T\"\ncomponents = [\"weather\"]\n").is_err());
        let err =
            parse("[[session.tabs]]\ntitle = \"T\"\nicon = \"no-such-icon\"\ncomponents = []\n")
                .unwrap_err();
        assert!(err.contains("no-such-icon"), "{err}");
        assert!(parse("[session]\ntabs = []\n").is_err());
    }

    #[test]
    fn what_the_environment_describes_can_be_placed_anywhere() {
        for subject in [Subject::Session, Subject::Environment, Subject::Github] {
            for c in [
                Component::Cpu,
                Component::Storage,
                Component::Environment,
                Component::Profiles,
            ] {
                assert!(c.describes(subject), "{c:?} {subject:?}");
            }
        }
        assert!(Component::Github.describes(Subject::Session));
        assert!(!Component::Github.describes(Subject::Environment));
        assert!(!Component::Files.describes(Subject::Github));
    }

    #[test]
    fn every_shipped_surface_describes_its_subject() {
        let shipped = [
            (Surface::session(), Subject::Session),
            (Surface::environment(), Subject::Environment),
            (Surface::github(), Subject::Github),
        ];
        for (surface, subject) in shipped {
            surface.check(subject, "shipped").unwrap();
            assert!(surface.tabs.iter().all(|t| t.icon().is_some()));
        }
    }

    #[test]
    fn a_tab_or_a_component_moves_stopping_at_either_end() {
        let mut s = Surface::session();
        s.move_tab(0, 1);
        assert_eq!(s.tabs[1].title, "Overview");
        s.move_tab(1, -9);
        assert_eq!(s.tabs[0].title, "Overview");
        s.move_tab(4, 3);
        assert_eq!(s.tabs[4].title, "Resources");
        let names = |s: &Surface| -> Vec<&str> {
            s.tabs[0]
                .components
                .iter()
                .map(|p| p.component.name())
                .collect()
        };
        s.shift(Slot::Tab(0), 6, -4);
        assert_eq!(
            names(&s),
            [
                "session",
                "change",
                "running",
                "github",
                "pulls",
                "timeline",
                "timelines",
                "about"
            ]
        );
        s.shift(Slot::Tab(0), 0, 9);
        assert_eq!(
            names(&s),
            [
                "change",
                "running",
                "github",
                "pulls",
                "timeline",
                "timelines",
                "about",
                "session"
            ]
        );
    }

    /// A surface of one tab, holding the pull request, and no foot.
    fn one_tab() -> Surface {
        Surface {
            foot: Vec::new(),
            tabs: vec![DetailsTab::new(
                "G",
                Icon::BrandGithub,
                &[Placed::new(Component::Github)],
            )],
        }
    }

    #[test]
    fn tabs_and_components_are_added_and_taken_out_but_the_last_tab_stays() {
        let mut s = one_tab();
        s.add_tab("Mine");
        s.add(Slot::Tab(1), Component::Memory);
        s.add(Slot::Foot, Component::Profiles);
        assert_eq!(s.tabs[1].components, [Placed::new(Component::Memory)]);
        assert_eq!(s.foot, [Placed::new(Component::Profiles)]);
        s.remove(Slot::Foot, 0);
        s.remove(Slot::Foot, 3);
        assert!(s.foot.is_empty());
        s.remove_tab(0);
        s.remove_tab(0);
        assert_eq!(s.tabs.len(), 1);
        assert_eq!(s.tabs[0].title, "Mine");
    }

    #[test]
    fn a_component_is_put_where_it_is_dropped_and_carried_as_it_shows() {
        let names = |l: &[Placed]| -> Vec<&str> { l.iter().map(|p| p.component.name()).collect() };
        let mut s = one_tab();
        s.insert(Slot::Foot, 0, Component::Cpu);
        s.insert(Slot::Foot, 0, Component::Memory);
        s.insert(Slot::Foot, 9, Component::Storage);
        assert_eq!(names(&s.foot), ["memory", "cpu", "storage"]);
        s.carry(Slot::Foot, 0, Slot::Foot, 2);
        assert_eq!(names(&s.foot), ["cpu", "memory", "storage"]);
        s.foot[2].folded = Some(true);
        s.carry(Slot::Foot, 2, Slot::Tab(0), 0);
        assert_eq!(names(&s.foot), ["cpu", "memory"]);
        assert_eq!(names(&s.tabs[0].components), ["storage", "github"]);
        assert_eq!(s.tabs[0].components[0].folded, Some(true));
        s.carry(Slot::Foot, 0, Slot::Tab(7), 0);
        assert_eq!(names(&s.foot), ["cpu", "memory"]);
    }

    #[test]
    fn a_component_stands_beside_the_one_before_it_until_it_is_moved_or_that_one_goes() {
        let mut s = Surface::github();
        s.foot = [Component::Cpu, Component::Gpu, Component::Memory]
            .map(Placed::new)
            .to_vec();
        s.set_beside(Slot::Foot, 0, true);
        s.set_beside(Slot::Foot, 1, true);
        s.set_beside(Slot::Foot, 2, true);
        let beside = |s: &Surface| s.foot.iter().map(|p| p.beside).collect::<Vec<_>>();
        assert_eq!(
            beside(&s),
            [false, true, true],
            "the first has none before it"
        );
        s.carry(Slot::Foot, 2, Slot::Foot, 0);
        assert_eq!(beside(&s), [false, false, true]);
        s.remove(Slot::Foot, 1);
        assert_eq!(beside(&s), [false, false]);
        let text = toml::to_string(&DetailsLayout {
            github: Some(s.clone()),
            ..DetailsLayout::default()
        })
        .unwrap();
        assert_eq!(parse(&text).unwrap().github, Some(s));
    }

    #[test]
    fn a_section_heading_says_what_it_is_titled() {
        let layout = parse("[github]\nfoot = [{ component = \"section\", title = \"Usage\" }]\n[[github.tabs]]\ntitle = \"G\"\ncomponents = [\"github\"]\n").unwrap();
        let foot = layout.github.clone().unwrap().foot;
        assert_eq!(foot[0].title.as_deref(), Some("Usage"));
        let text = toml::to_string(&layout).unwrap();
        assert_eq!(parse(&text).unwrap(), layout);
    }

    #[test]
    fn a_button_says_what_its_placement_says_or_else_what_adroit_has_it_say() {
        let layout = parse(
            "[github]\nfoot = [{ component = \"github-open\", title = \"On the web\", icon = \"\", icon-only = true, tone = \"primary\" }]\n[[github.tabs]]\ntitle = \"G\"\ncomponents = [\"github-start\"]\n",
        )
        .unwrap();
        let surface = layout.github.clone().unwrap();
        let open = &surface.foot[0];
        assert_eq!(open.label(), "On the web");
        assert_eq!(open.button_icon(), None);
        assert!(open.icon_only);
        assert_eq!(open.tone(), Tone::Primary);
        let start = &surface.tabs[0].components[0];
        assert_eq!(start.label(), "Start a session");
        assert_eq!(start.button_icon(), Some(Icon::MessagePlus));
        assert_eq!(start.tone(), Tone::Plain);
        let text = toml::to_string(&layout).unwrap();
        assert_eq!(parse(&text).unwrap(), layout);
    }

    #[test]
    fn the_session_timelines_take_in_the_sessions_tied_to_the_one_shown_unless_placed_wider() {
        let layout = parse(
            "[[session.tabs]]\ntitle = \"T\"\n\
             components = [\"timelines\", { component = \"timelines\", scope = \"workspace\" }, { component = \"timelines\", scope = \"environment\" }]\n",
        )
        .unwrap();
        let surface = layout.session.clone().unwrap();
        let scopes: Vec<Scope> = surface.tabs[0]
            .components
            .iter()
            .map(Placed::scope)
            .collect();
        assert_eq!(
            scopes,
            [Scope::Related, Scope::Workspace, Scope::Environment]
        );
        let text = toml::to_string(&layout).unwrap();
        assert_eq!(parse(&text).unwrap(), layout);
        assert!(
            parse("[[session.tabs]]\ntitle = \"T\"\ncomponents = [{ component = \"timelines\", scope = \"galaxy\" }]\n")
                .is_err()
        );
    }

    #[test]
    fn every_component_has_its_own_icon_and_a_line_on_what_it_shows() {
        let icons: std::collections::HashSet<_> = Component::ALL.iter().map(|c| c.icon()).collect();
        assert_eq!(icons.len(), Component::ALL.len());
        assert!(Component::ALL.iter().all(|c| c.about().ends_with('.')));
    }

    #[test]
    fn the_components_offered_for_a_subject_are_those_that_can_describe_it() {
        let offered: Vec<Component> = Component::placeable(Subject::Github).collect();
        assert_eq!(
            offered,
            [
                Component::Section,
                Component::Cpu,
                Component::Gpu,
                Component::Memory,
                Component::Storage,
                Component::Environment,
                Component::EnvironmentRetry,
                Component::EnvironmentPair,
                Component::EnvironmentSettings,
                Component::EnvironmentWaiting,
                Component::Profiles,
                Component::Github,
                Component::GithubShow,
                Component::GithubOpen,
                Component::GithubStart,
                Component::GithubConversation,
                Component::GithubFiles,
                Component::GithubChecks
            ]
        );
        assert_eq!(
            Component::placeable(Subject::Session).count(),
            Component::ALL.len()
        );
    }
}

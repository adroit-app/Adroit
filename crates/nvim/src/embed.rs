use std::sync::{Arc, Mutex};

use nvim_rs::compat::tokio::Compat;
use nvim_rs::error::CallError;
use nvim_rs::{Handler, Neovim, UiAttachOptions};
use rmpv::Value;
use tokio::process::{Child, ChildStdin, Command};

use crate::UiState;

type Writer = Compat<ChildStdin>;
pub(crate) type Error = Box<dyn std::error::Error + Send + Sync>;

/// What the embedded Neovim reports to the window.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A full frame has been drawn.
    Redraw,
    /// The prompt needs this many screen lines to show all of its text.
    Height(usize),
    /// The prompt shows from this line, counted from 0, of so many: where it
    /// is scrolled to when its text is taller than it.
    View { top: usize, lines: usize },
    /// The reader yanked `text`, of so many lines, for the window to copy.
    Yank { text: String, lines: usize },
    /// The reader shows from this line, and this column, counted from 0.
    Scrolled { top: usize, left: usize },
    /// The prompt's first character became `!`, or stopped being one.
    Shell(bool),
    /// A `/` was typed where it offers commands.
    CommandsWanted,
    /// The prompt needs directory entries from the work host.
    PathsWanted {
        id: u64,
        root: String,
        typed: String,
    },
    /// A key or a command of the prompt asked for adroit's action, named as
    /// a key binding names it.
    Action(String),
    /// The user's configuration was saved, for the window to start Neovim
    /// again on it.
    ConfigurationSaved,
    /// The prompt's Neovim is about to quit, holding `text` in `view`, for
    /// the Neovim started next to hold.
    Leaving { text: String, view: View },
}

const TAKE_TEXT: &str = include_str!("../lua/take_text.lua");

const APPEND: &str = include_str!("../lua/append.lua");

const TEXT: &str = include_str!("../lua/text.lua");

const CURSOR: &str = include_str!("../lua/cursor.lua");

const SET_CURSOR: &str = include_str!("../lua/set_cursor.lua");

const VIEW: &str = include_str!("../lua/view.lua");

const SET_VIEW: &str = include_str!("../lua/set_view.lua");

#[derive(Clone)]
struct Listener {
    state: Arc<Mutex<UiState>>,
    events: Arc<dyn Fn(Event) + Send + Sync>,
}

#[async_trait::async_trait]
impl Handler for Listener {
    type Writer = Writer;

    /// What Neovim waits for an answer to: told as it quits, so that what it
    /// tells is taken before it ends.
    async fn handle_request(
        &self,
        name: String,
        args: Vec<Value>,
        _nvim: Neovim<Writer>,
    ) -> Result<Value, Value> {
        match (name.as_str(), args.first().and_then(Value::as_str)) {
            ("adroit_leaving", Some(text)) => {
                let view = View(args.get(1).cloned().unwrap_or(Value::Nil));
                (self.events)(Event::Leaving {
                    text: text.to_string(),
                    view,
                });
                Ok(Value::Nil)
            }
            _ => Err(Value::from(format!("adroit answers no {name}"))),
        }
    }

    async fn handle_notify(&self, name: String, args: Vec<Value>, _nvim: Neovim<Writer>) {
        match name.as_str() {
            "redraw" => {
                let flushed = args.iter().any(|batch| {
                    batch
                        .as_array()
                        .and_then(|b| b.first())
                        .and_then(Value::as_str)
                        == Some("flush")
                });
                self.state.lock().unwrap().apply(&args);
                if flushed {
                    (self.events)(Event::Redraw);
                }
            }
            "adroit_view" => {
                let at = |i: usize| args.get(i).and_then(Value::as_u64).map(|n| n as usize);
                if let (Some(top), Some(lines)) = (at(0), at(1)) {
                    (self.events)(Event::View { top, lines });
                }
            }
            "adroit_scrolled" => {
                let at = |i: usize| args.get(i).and_then(Value::as_u64).map(|n| n as usize);
                if let (Some(top), Some(left)) = (at(0), at(1)) {
                    (self.events)(Event::Scrolled { top, left });
                }
            }
            "adroit_yank" => {
                let text = args.first().and_then(Value::as_str).map(str::to_string);
                let lines = args.get(1).and_then(Value::as_u64).unwrap_or(1) as usize;
                if let Some(text) = text {
                    (self.events)(Event::Yank { text, lines });
                }
            }
            "adroit_shell" => {
                if let Some(on) = args.first().and_then(Value::as_bool) {
                    (self.events)(Event::Shell(on));
                }
            }
            "adroit_commands_wanted" => (self.events)(Event::CommandsWanted),
            "adroit_paths_wanted" => {
                if let (Some(id), Some(root), Some(typed)) = (
                    args.first().and_then(Value::as_u64),
                    args.get(1).and_then(Value::as_str),
                    args.get(2).and_then(Value::as_str),
                ) {
                    (self.events)(Event::PathsWanted {
                        id,
                        root: root.into(),
                        typed: typed.into(),
                    });
                }
            }
            "adroit_action" => {
                if let Some(action) = args.first().and_then(Value::as_str) {
                    (self.events)(Event::Action(action.to_string()));
                }
            }
            "adroit_configuration_saved" => (self.events)(Event::ConfigurationSaved),
            "adroit_height" => {
                if let Some(n) = args.first().and_then(Value::as_u64) {
                    (self.events)(Event::Height(n as usize));
                }
            }
            _ => {}
        }
    }
}

/// Something the prompt offers: a session or a file after `@`, a command
/// after `/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub word: String,
    /// What it is, shown beside it.
    pub menu: String,
    /// More about it, shown while it is picked.
    pub info: String,
}

impl Candidate {
    pub fn new(word: impl Into<String>, menu: impl Into<String>) -> Self {
        Self {
            word: word.into(),
            menu: menu.into(),
            info: String::new(),
        }
    }

    pub fn with_info(mut self, info: impl Into<String>) -> Self {
        self.info = info.into();
        self
    }
}

fn candidate_list(candidates: &[Candidate]) -> Value {
    let list = candidates
        .iter()
        .map(|c| {
            Value::Map(vec![
                (Value::from("word"), Value::from(c.word.as_str())),
                (Value::from("menu"), Value::from(c.menu.as_str())),
                (Value::from("info"), Value::from(c.info.as_str())),
            ])
        })
        .collect();
    Value::Array(list)
}

/// The name the prompt's Neovim runs under, so its configuration, plugins
/// and state are found in folders of their own, apart from the everyday
/// Neovim's and the console reader's.
pub(crate) const PROMPT_APPNAME: &str = "adroit/prompt/nvim";

/// How one of adroit's Neovims is started.
pub(crate) struct Launch {
    /// The name it runs under, which finds its configuration, plugins and
    /// state in folders of their own.
    pub(crate) appname: &'static str,
    /// The module of adroit's required before the user's configuration,
    /// which may change anything it sets.
    pub(crate) defaults: &'static str,
    /// The module of adroit's set up once startup has finished, given the
    /// window's channel, which holds what the window relies on.
    pub(crate) setup: &'static str,
}

/// A configuration of the user's: `init.lua` in a folder of its own, kept
/// until it is dropped.
#[cfg(test)]
pub(crate) struct UserConfig(tempfile::TempDir);

#[cfg(test)]
impl UserConfig {
    pub(crate) fn new(lua: &str) -> Self {
        let config = Self(tempfile::tempdir().unwrap());
        config.write(lua);
        config
    }

    pub(crate) fn init(&self) -> std::path::PathBuf {
        self.0.path().join("init.lua")
    }

    /// Save `init.lua` anew, as an editor saves it.
    pub(crate) fn write(&self, lua: &str) {
        std::fs::write(self.init(), lua).unwrap();
    }

    /// What starts Neovim on it alone: read as the user's own, as it is
    /// found when Neovim looks for one.
    pub(crate) fn args(&self) -> Vec<String> {
        let init = self.init().to_string_lossy().into_owned();
        vec![
            "--clean".into(),
            "-u".into(),
            init.clone(),
            "--cmd".into(),
            format!("let $MYVIMRC = '{init}'"),
        ]
    }
}

/// Where a Neovim's window was, its cursor and what it was scrolled to, kept
/// to be put in another Neovim showing the same text.
#[derive(Debug, Clone, PartialEq)]
pub struct View(Value);

/// Save with `save` until `rx` tells of the configuration saved: a watch
/// is started as the Neovim starts, and the first saves may come before it.
#[cfg(test)]
pub(crate) async fn save_until_told(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<Event>,
    save: impl Fn(),
) {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            save();
            let told = tokio::time::timeout(std::time::Duration::from_millis(500), async {
                while let Some(ev) = rx.recv().await {
                    if ev == Event::ConfigurationSaved {
                        return;
                    }
                }
            });
            if told.await.is_ok() {
                return;
            }
        }
    })
    .await
    .expect("never told");
}

/// A Neovim process attached as this window's UI.
pub struct Embedded {
    nvim: Neovim<Writer>,
    state: Arc<Mutex<UiState>>,
    input: adroit_term::InputPermission,
    _child: Child,
}

impl Embedded {
    /// Start `program` with `args` and attach to it at `cols` × `rows`.
    /// `events` runs on the Neovim I/O task.
    pub async fn spawn(
        program: &str,
        args: &[&str],
        cols: u16,
        rows: u16,
        events: impl Fn(Event) + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        let launch = Launch {
            appname: PROMPT_APPNAME,
            defaults: "adroit.prompt_defaults",
            setup: "adroit.prompt",
        };
        Self::spawn_with(program, args, cols, rows, launch, events).await
    }

    /// Start `program` as [`Embedded::spawn`] does, as `launch` has it.
    pub(crate) async fn spawn_with(
        program: &str,
        args: &[&str],
        cols: u16,
        rows: u16,
        launch: Launch,
        events: impl Fn(Event) + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        let state = Arc::new(Mutex::new(UiState::default()));
        let listener = Listener {
            state: state.clone(),
            events: Arc::new(events),
        };
        let mut cmd = Command::new(program);
        cmd.arg("--embed")
            .env("NVIM_APPNAME", launch.appname)
            .args(["--cmd", "let g:adroit = v:true"])
            .args([
                "--cmd",
                &format!(
                    "lua {}; require('adroit.config').prepare()",
                    crate::lua::require(launch.defaults)
                ),
            ])
            .args(args)
            .kill_on_drop(true);
        let (nvim, _io, child) = nvim_rs::create::tokio::new_child_cmd(&mut cmd, listener).await?;
        let mut opts = UiAttachOptions::new();
        opts.set_rgb(true)
            .set_linegrid_external(true)
            .set_popupmenu_external(true)
            .set_cmdline_external(true)
            .set_messages_externa(true);
        nvim.ui_attach(cols as i64, rows as i64, &opts).await?;
        let info = nvim.get_api_info().await?;
        let channel = info.first().and_then(Value::as_i64).unwrap_or(0);
        let setup = format!("require({:?}).setup(...)", launch.setup);
        nvim.exec_lua(&setup, vec![Value::from(channel)]).await?;
        Ok(Self {
            nvim,
            state,
            input: adroit_term::InputPermission::default(),
            _child: child,
        })
    }

    /// Ask Neovim with `request` on a task of its own, which waits for the
    /// answer even when the caller stops waiting. nvim-rs ends the whole
    /// connection when an answer comes for a request whose future was
    /// dropped, and a caller's future is dropped with the component that
    /// spawned it.
    async fn ask<T, F>(&self, request: impl FnOnce(Neovim<Writer>) -> F) -> Result<T, Error>
    where
        T: Send + 'static,
        F: Future<Output = Result<T, Box<CallError>>> + Send + 'static,
    {
        Ok(tokio::spawn(request(self.nvim.clone())).await??)
    }

    #[cfg(test)]
    pub(crate) async fn stdpath(&self, kind: &str) -> String {
        let kind = Value::from(kind);
        let path = self
            .ask(move |n| async move { n.call_function("stdpath", vec![kind]).await })
            .await
            .unwrap();
        path.as_str().unwrap().to_string()
    }

    #[cfg(test)]
    pub(crate) async fn eval(&self, expr: &str) -> Value {
        let expr = expr.to_string();
        self.ask(move |n| async move { n.eval(&expr).await })
            .await
            .unwrap()
    }

    /// Wait, at most two seconds, for `expr` to be `want`.
    #[cfg(test)]
    pub(crate) async fn settles(&self, expr: &str, want: Value) -> bool {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while self.eval(expr).await != want {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .is_ok()
    }

    async fn exec_lua(&self, lua: &str, args: Vec<Value>) -> Result<Value, Error> {
        let lua = lua.to_string();
        self.ask(move |n| async move { n.exec_lua(&lua, args).await })
            .await
    }

    pub(crate) async fn exec(&self, lua: &str, args: Vec<Value>) -> Result<(), Error> {
        self.exec_lua(lua, args).await?;
        Ok(())
    }

    pub fn state(&self) -> std::sync::MutexGuard<'_, UiState> {
        self.state.lock().unwrap()
    }

    /// Type keys in Neovim's notation.
    pub async fn input(&self, keys: &str) -> Result<(), Error> {
        let Some(ticket) = self.input.ticket() else {
            return Ok(());
        };
        let input = self.input.clone();
        self.state().forget_message();
        let keys = keys.to_string();
        self.ask(move |n| async move {
            if !input.allows(ticket) {
                return Ok(0);
            }
            n.input(&keys).await
        })
        .await?;
        Ok(())
    }

    pub async fn resize(&self, cols: u16, rows: u16) -> Result<(), Error> {
        self.ask(move |n| async move { n.ui_try_resize(cols as i64, rows as i64).await })
            .await
    }

    /// Type keys in Neovim's notation as Neovim's own keys, whatever the user
    /// mapped them to, in order with the keys typed before and after them.
    pub async fn press(&self, keys: &str) -> Result<(), Error> {
        // `<Cmd>` cannot be mapped, and the keys it feeds go right after it
        // without mappings; Neovim's API has no input that skips mappings.
        let keys = keys.replace('\'', "''").replace('<', "<lt>");
        self.input(&format!(
            "<Cmd>call feedkeys(nvim_replace_termcodes('{keys}', v:true, v:false, v:true), 'ni')<CR>"
        ))
        .await
    }

    /// Leave whatever mode the prompt is in and start writing after its text.
    pub async fn start_writing(&self) -> Result<(), Error> {
        self.press("<C-\\><C-N>GA").await
    }

    /// Go back to normal mode, from whatever mode the prompt is in.
    pub async fn stop_writing(&self) -> Result<(), Error> {
        self.press("<C-\\><C-N>").await
    }

    /// Type text as if from the keyboard: a `<` in it is a `<`, not the start
    /// of a key name.
    pub async fn type_text(&self, text: &str) -> Result<(), Error> {
        self.input(&text.replace('<', "<lt>")).await
    }

    /// Put text into the prompt as text, whatever mode it is in.
    pub async fn paste(&self, text: &str) -> Result<(), Error> {
        let Some(ticket) = self.input.ticket() else {
            return Ok(());
        };
        let input = self.input.clone();
        self.state().forget_message();
        let text = text.to_string();
        self.ask(move |n| async move {
            if !input.allows(ticket) {
                return Ok(false);
            }
            n.paste(&text, false, -1).await
        })
        .await?;
        Ok(())
    }

    async fn set_var(&self, name: &'static str, value: Value) -> Result<(), Error> {
        self.ask(move |n| async move { n.set_var(name, value).await })
            .await
    }

    /// Replace what `@` in the prompt offers.
    pub async fn set_candidates(&self, candidates: &[Candidate]) -> Result<(), Error> {
        self.set_var("adroit_candidates", candidate_list(candidates))
            .await
    }

    /// Replace what `/` in the prompt offers; none offers nothing.
    pub async fn set_commands(&self, commands: &[Candidate]) -> Result<(), Error> {
        self.set_var("adroit_commands", candidate_list(commands))
            .await
    }

    /// The directory a path after `@` is walked from; without one, a path is
    /// walked only from the root of the disk.
    pub async fn set_root(&self, root: Option<&std::path::Path>) -> Result<(), Error> {
        let root = root
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.exec(
            "require('adroit.prompt').root(...)",
            vec![Value::from(root)],
        )
        .await
        .map(|_| ())
    }

    pub async fn set_paths(&self, id: u64, paths: &[Candidate]) -> Result<(), Error> {
        self.exec(
            "require('adroit.prompt').paths(...)",
            vec![Value::from(id), candidate_list(paths)],
        )
        .await
        .map(|_| ())
    }

    /// Tell Neovim whether the window's theme is light, so its colour
    /// scheme draws text that shows on the window's background.
    pub async fn set_light(&self, light: bool) -> Result<(), Error> {
        let background = Value::from(if light { "light" } else { "dark" });
        self.exec(
            "vim.g.adroit_background = ...; if vim.o.background ~= ... then vim.o.background = ... end",
            vec![background],
        )
        .await
    }

    /// The prompt's text.
    pub async fn text(&self) -> Result<String, Error> {
        self.exec_string(TEXT).await
    }

    pub async fn freeze_prompt(&self, frozen: bool) -> Result<(), Error> {
        if frozen {
            self.input.set(false);
        }
        let result = self
            .exec(
                "require('adroit.prompt').freeze(...)",
                vec![Value::from(frozen)],
            )
            .await;
        if !frozen && result.is_ok() {
            self.input.set(true);
        }
        result
    }

    pub async fn consume_text(&self, text: &str) -> Result<bool, Error> {
        self.exec_lua(
            "return require('adroit.prompt').consume(...)",
            vec![Value::from(text)],
        )
        .await
        .map(|value| value.as_bool().unwrap_or(false))
    }

    /// Add lines after what the prompt holds, the cursor left where it is in
    /// that, or in its place when it is empty, with the cursor at their end.
    pub async fn append(&self, lines: &[String]) -> Result<(), Error> {
        let list = lines.iter().map(|l| Value::from(l.as_str())).collect();
        self.state().forget_message();
        self.exec(APPEND, vec![Value::Array(list)]).await
    }

    /// Where the cursor is in the prompt's text: the line counted from 1 and
    /// the byte from 0.
    pub async fn cursor(&self) -> Result<(usize, usize), Error> {
        let at = self.exec_lua(CURSOR, vec![]).await?;
        let n = |i: usize| {
            at.as_array()
                .and_then(|a| a.get(i))
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize
        };
        Ok((n(0).max(1), n(1)))
    }

    /// Put the cursor at `at` in the prompt's text, or as near as the text
    /// lets it.
    pub async fn set_cursor(&self, (line, byte): (usize, usize)) -> Result<(), Error> {
        self.exec(
            SET_CURSOR,
            vec![Value::from(line as u64), Value::from(byte as u64)],
        )
        .await
    }

    /// The prompt's text, leaving the prompt empty for the next one.
    pub async fn take_text(&self) -> Result<String, Error> {
        self.state().forget_message();
        self.exec_string(TAKE_TEXT).await
    }

    /// Put `text` in a prompt just started, leaving the message it started
    /// with shown, as one a configuration failed with must stay seen.
    pub async fn put_back(&self, text: &str) -> Result<(), Error> {
        let list = text.split('\n').map(Value::from).collect();
        self.exec(APPEND, vec![Value::Array(list)]).await
    }

    /// Where the window is: its cursor and what it is scrolled to.
    pub async fn view(&self) -> Result<View, Error> {
        Ok(View(self.exec_lua(VIEW, vec![]).await?))
    }

    /// Put the window where `view` has it, or as near as the text lets it.
    pub async fn set_view(&self, view: &View) -> Result<(), Error> {
        self.exec(SET_VIEW, vec![view.0.clone()]).await
    }

    /// Have Neovim quit, whatever is left unsaved in it, once it has answered.
    pub async fn quit(&self) -> Result<(), Error> {
        self.exec("vim.schedule(function() vim.cmd('qall!') end)", vec![])
            .await
    }

    /// The string `code` returns; empty for anything else.
    async fn exec_string(&self, code: &str) -> Result<String, Error> {
        Ok(self
            .exec_lua(code, vec![])
            .await?
            .as_str()
            .unwrap_or_default()
            .to_string())
    }
}

#[cfg(test)]
mod tests;

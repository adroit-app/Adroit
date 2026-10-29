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
mod tests {
    use std::time::Duration;

    use tokio::sync::mpsc;

    use super::*;

    async fn start() -> (Embedded, mpsc::UnboundedReceiver<Event>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let e = Embedded::spawn(&crate::program(), &["--clean"], 30, 4, move |ev| {
            let _ = tx.send(ev);
        })
        .await
        .expect("nvim on PATH");
        attached(&e).await;
        (e, rx)
    }

    /// Wait until the prompt's language server is attached, as it is long
    /// before anyone types.
    async fn attached(e: &Embedded) {
        assert!(
            e.settles(
                "luaeval('vim.iter(vim.lsp.get_clients({ name = \"adroit\" })):any(function(c) return c.initialized end)')",
                Value::from(true)
            )
            .await
        );
    }

    /// Wait until `done` holds, looking again after each event.
    async fn until(rx: &mut mpsc::UnboundedReceiver<Event>, done: impl Fn() -> bool) {
        tokio::time::timeout(Duration::from_secs(10), async {
            while !done() {
                rx.recv().await;
            }
        })
        .await
        .expect("timed out");
    }

    async fn wait(rx: &mut mpsc::UnboundedReceiver<Event>, done: impl Fn(&Event) -> bool) {
        tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(ev) = rx.recv().await {
                if done(&ev) {
                    return;
                }
            }
        })
        .await
        .expect("timed out");
    }

    async fn start_configured(
        init: &str,
    ) -> (Embedded, mpsc::UnboundedReceiver<Event>, UserConfig) {
        let config = UserConfig::new(init);
        let args = config.args();
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let (tx, rx) = mpsc::unbounded_channel();
        let e = Embedded::spawn(&crate::program(), &args, 30, 4, move |ev| {
            let _ = tx.send(ev);
        })
        .await
        .expect("nvim on PATH");
        attached(&e).await;
        (e, rx, config)
    }

    /// How many times the configuration was told saved over `within`.
    async fn saves_told(rx: &mut mpsc::UnboundedReceiver<Event>, within: Duration) -> usize {
        let mut told = 0;
        let _ = tokio::time::timeout(within, async {
            while let Some(ev) = rx.recv().await {
                if ev == Event::ConfigurationSaved {
                    told += 1;
                }
            }
        })
        .await;
        told
    }

    #[tokio::test]
    async fn saving_the_configuration_asks_for_a_restart_and_reads_nothing_itself() {
        let (e, mut rx, config) = start_configured("vim.g.read = 1").await;
        assert!(e.settles("g:read", Value::from(1)).await);
        save_until_told(&mut rx, || config.write("vim.g.read = 2")).await;
        assert_eq!(e.eval("g:read").await, Value::from(1));
    }

    #[tokio::test]
    async fn an_editor_s_save_of_several_writes_asks_for_one_restart() {
        let (_e, mut rx, config) = start_configured("").await;
        save_until_told(&mut rx, || config.write("")).await;
        // The last save's own events, told once.
        saves_told(&mut rx, Duration::from_millis(300)).await;
        for n in 0..3 {
            config.write(&format!("vim.g.read = {n}"));
        }
        assert_eq!(saves_told(&mut rx, Duration::from_secs(1)).await, 1);
    }

    #[tokio::test]
    async fn saving_a_lua_module_of_the_configuration_asks_for_a_restart_too() {
        let config = UserConfig::new("");
        let lua = config.init().with_file_name("lua");
        std::fs::create_dir_all(&lua).unwrap();
        std::fs::write(lua.join("mine.lua"), "return { n = 1 }").unwrap();
        let args = config.args();
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let _e = Embedded::spawn(&crate::program(), &args, 30, 4, move |ev| {
            let _ = tx.send(ev);
        })
        .await
        .unwrap();
        save_until_told(&mut rx, || {
            std::fs::write(lua.join("mine.lua"), "return { n = 2 }").unwrap()
        })
        .await;
    }

    #[tokio::test]
    async fn a_lua_module_in_a_folder_made_after_starting_asks_for_a_restart_too() {
        let (_e, mut rx, config) = start_configured("").await;
        save_until_told(&mut rx, || config.write("")).await;
        let deep = config.init().with_file_name("lua").join("deep");
        std::fs::create_dir_all(&deep).unwrap();
        wait(&mut rx, |ev| *ev == Event::ConfigurationSaved).await;
        save_until_told(&mut rx, || {
            std::fs::write(deep.join("mine.lua"), "return { n = 1 }").unwrap()
        })
        .await;
    }

    #[tokio::test]
    async fn the_text_and_the_view_of_one_prompt_can_be_put_in_the_next() {
        let (old, mut rx) = start().await;
        old.input("one<CR>two<CR>three<Esc>kh").await.unwrap();
        until(&mut rx, || old.state().mode() == "normal").await;
        let text = old.text().await.unwrap();
        let view = old.view().await.unwrap();
        let at = old.cursor().await.unwrap();
        old.quit().await.unwrap();
        let (next, _rx) = start().await;
        next.put_back(&text).await.unwrap();
        next.set_view(&view).await.unwrap();
        assert_eq!(next.text().await.unwrap(), "one\ntwo\nthree");
        assert_eq!(next.cursor().await.unwrap(), at);
    }

    #[tokio::test]
    async fn putting_the_text_back_leaves_what_the_configuration_said_at_start() {
        let (e, mut rx, _config) = start_configured("error('broken')").await;
        until(&mut rx, || {
            e.state().message().is_some_and(|m| m.contains("broken"))
        })
        .await;
        e.put_back("kept").await.unwrap();
        assert_eq!(e.text().await.unwrap(), "kept");
        assert!(e.state().message().is_some_and(|m| m.contains("broken")));
    }

    #[tokio::test]
    async fn a_neovim_asked_to_quit_ends_what_it_tells() {
        let (e, mut rx) = start().await;
        e.quit().await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            while rx.recv().await.is_some() {}
        })
        .await
        .expect("still running");
    }

    #[tokio::test]
    async fn quitting_tells_what_the_prompt_holds_before_it_ends() {
        for quit in ["<Esc>:q<CR>", "<Esc>ZQ", "<Esc>:qa!<CR>"] {
            let (e, mut rx) = start().await;
            e.type_text("kept").await.unwrap();
            wait(&mut rx, |_| e.state().screen().text()[0] == "kept").await;
            e.input(quit).await.unwrap();
            let mut told = None;
            tokio::time::timeout(Duration::from_secs(10), async {
                while let Some(ev) = rx.recv().await {
                    if let Event::Leaving { text, .. } = ev {
                        told = Some(text);
                    }
                }
            })
            .await
            .expect("still running");
            assert_eq!(told.as_deref(), Some("kept"), "{quit}");
        }
    }

    #[tokio::test]
    async fn it_keeps_its_configuration_and_state_apart_from_the_everyday_neovim() {
        let (e, _rx) = start().await;
        for kind in ["config", "data", "state"] {
            let path = e.stdpath(kind).await;
            assert!(path.ends_with("adroit/prompt/nvim"), "{kind}: {path}");
        }
    }

    #[tokio::test]
    async fn the_user_s_configuration_changes_what_adroit_only_suggests() {
        let (e, _rx, _config) = start_configured(
            r#"
            vim.o.number = true
            vim.o.cursorline = true
            vim.o.completeopt = "menu"
            vim.api.nvim_create_autocmd("FileType", {
              pattern = "adroit-prompt",
              callback = function() vim.wo.wrap = false end,
            })
            "#,
        )
        .await;
        assert!(e.settles("&number", Value::from(1)).await);
        assert!(e.settles("&cursorline", Value::from(1)).await);
        assert!(e.settles("&completeopt", Value::from("menu")).await);
        assert!(e.settles("&wrap", Value::from(0)).await);
    }

    #[tokio::test]
    async fn the_user_s_configuration_cannot_change_what_the_window_relies_on() {
        let (e, _rx, _config) = start_configured(
            r#"
            vim.o.laststatus = 2
            vim.o.showtabline = 2
            vim.o.cmdheight = 1
            vim.o.eventignore = "TextChanged"
            vim.o.winbar = "bar"
            vim.o.modifiable = false
            vim.api.nvim_create_autocmd("FileType", {
              pattern = "adroit-prompt",
              callback = function() vim.bo.buftype = "" end,
            })
            "#,
        )
        .await;
        let held = "[&laststatus, &showtabline, &cmdheight, &eventignore, \
                    &winbar, &modifiable, &buftype, &filetype]";
        let want = Value::from(vec![
            Value::from(0),
            Value::from(0),
            Value::from(0),
            Value::from(""),
            Value::from(""),
            Value::from(1),
            Value::from("nofile"),
            Value::from("adroit-prompt"),
        ]);
        assert!(e.settles(held, want.clone()).await);
        e.exec(
            "vim.o.laststatus = 2; vim.bo.buftype = ''; vim.bo.filetype = 'text'",
            vec![],
        )
        .await
        .unwrap();
        assert!(e.settles(held, want).await);
    }

    #[tokio::test]
    async fn writing_starts_after_the_text_whatever_the_user_mapped_its_keys_to() {
        let (e, mut rx, _config) = start_configured(
            r#"
            vim.keymap.set({ "n", "i" }, "<C-\\>", "<Nop>")
            vim.keymap.set("n", "G", "<Nop>")
            vim.keymap.set("n", "A", "<Nop>")
            "#,
        )
        .await;
        e.input("one<CR>two<Esc>k0").await.unwrap();
        until(&mut rx, || e.state().mode() == "normal").await;
        e.start_writing().await.unwrap();
        e.input("!").await.unwrap();
        until(&mut rx, || e.state().screen().text()[1] == "two!").await;
        e.stop_writing().await.unwrap();
        until(&mut rx, || e.state().mode() == "normal").await;
    }

    #[tokio::test]
    async fn the_theme_s_background_is_kept_whatever_changes_it() {
        let (e, _rx) = start().await;
        e.set_light(true).await.unwrap();
        e.exec("vim.o.background = 'dark'", vec![]).await.unwrap();
        assert!(e.settles("&background", Value::from("light")).await);
    }

    #[tokio::test]
    async fn a_light_theme_draws_the_text_dark() {
        let (e, mut rx) = start().await;
        e.input("hello").await.unwrap();
        wait(&mut rx, |_| e.state().screen().text()[0] == "hello").await;
        let lum = |c: adroit_term::Rgb| c.r as u32 + c.g as u32 + c.b as u32;
        let fg = |e: &Embedded| e.state().screen().lines[0].runs[0].style.fg;
        e.set_light(true).await.unwrap();
        wait(&mut rx, |_| lum(fg(&e)) < 3 * 128).await;
        e.set_light(false).await.unwrap();
        wait(&mut rx, |_| lum(fg(&e)) >= 3 * 128).await;
    }

    #[tokio::test]
    async fn typed_text_is_drawn_and_can_be_taken() {
        let (e, mut rx) = start().await;
        e.input("hello").await.unwrap();
        wait(&mut rx, |_| e.state().screen().text()[0] == "hello").await;
        assert_eq!(e.take_text().await.unwrap(), "hello");
        assert_eq!(e.text().await.unwrap(), "");
    }

    #[tokio::test]
    async fn the_prompt_starts_in_insert_mode_without_a_status_line() {
        let (e, mut rx) = start().await;
        wait(&mut rx, |_| e.state().mode().starts_with("insert")).await;
        let s = e.state().screen();
        assert_eq!(s.rows, 4);
        assert!(s.text().iter().all(|l| l.is_empty()), "{:?}", s.text());
    }

    #[tokio::test]
    async fn writing_starts_after_what_the_prompt_holds_from_any_mode() {
        let (e, mut rx) = start().await;
        e.input("one<CR>two<Esc>gg0").await.unwrap();
        until(&mut rx, || e.state().mode() == "normal").await;
        e.start_writing().await.unwrap();
        until(&mut rx, || e.state().mode().starts_with("insert")).await;
        e.input("!").await.unwrap();
        until(&mut rx, || e.state().screen().text()[1] == "two!").await;
        e.input("<Esc>:").await.unwrap();
        until(&mut rx, || e.state().mode() == "cmdline_normal").await;
        e.start_writing().await.unwrap();
        until(&mut rx, || e.state().mode().starts_with("insert")).await;
    }

    #[tokio::test]
    async fn stopping_writing_leaves_normal_mode_from_any_mode() {
        let (e, mut rx) = start().await;
        e.start_writing().await.unwrap();
        until(&mut rx, || e.state().mode().starts_with("insert")).await;
        e.stop_writing().await.unwrap();
        until(&mut rx, || e.state().mode() == "normal").await;
        e.input(":").await.unwrap();
        until(&mut rx, || e.state().mode() == "cmdline_normal").await;
        e.stop_writing().await.unwrap();
        until(&mut rx, || e.state().mode() == "normal").await;
    }

    #[tokio::test]
    async fn a_prompt_taller_than_its_window_reports_where_it_is_scrolled_to() {
        let (e, mut rx) = start().await;
        e.input("1<CR>2<CR>3<CR>4<CR>5<CR>6<CR>7").await.unwrap();
        // Four rows show 4 to 7.
        wait(&mut rx, |ev| *ev == Event::View { top: 3, lines: 7 }).await;
        e.input("<Esc>gg").await.unwrap();
        wait(&mut rx, |ev| *ev == Event::View { top: 0, lines: 7 }).await;
    }

    #[tokio::test]
    async fn a_new_line_reports_the_height_the_text_needs() {
        let (e, mut rx) = start().await;
        e.input("a<CR>b<CR>c").await.unwrap();
        wait(&mut rx, |ev| *ev == Event::Height(3)).await;
    }

    #[tokio::test]
    async fn a_prompt_grown_to_fit_a_paste_shows_it_from_the_first_line() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let e = Embedded::spawn(&crate::program(), &["--clean"], 30, 2, move |ev| {
            let _ = tx.send(ev);
        })
        .await
        .expect("nvim on PATH");
        // As the window does: a prompt of one line is resized down to it.
        e.resize(30, 1).await.unwrap();
        until(&mut rx, || e.state().screen().rows == 1).await;
        e.paste("one\ntwo").await.unwrap();
        wait(&mut rx, |ev| *ev == Event::Height(2)).await;
        // A one-line window shows the line the cursor is on.
        e.input("<Esc>").await.unwrap();
        wait(&mut rx, |_| e.state().mode().starts_with("normal")).await;
        assert_eq!(e.state().screen().text(), ["two"]);
        e.resize(30, 2).await.unwrap();
        until(&mut rx, || e.state().screen().text() == ["one", "two"]).await;
    }

    #[tokio::test]
    async fn a_line_that_wraps_with_the_menu_open_reports_its_height() {
        let (e, mut rx) = start().await;
        e.set_candidates(&[Candidate::new("auth/web", "session")])
            .await
            .unwrap();
        e.input("abcdefghijklmnopqrstuvwxyz @a").await.unwrap();
        wait(&mut rx, |_| e.state().popup().is_some()).await;
        e.input("ut").await.unwrap();
        wait(&mut rx, |ev| *ev == Event::Height(2)).await;
    }
    #[tokio::test]
    async fn an_at_sign_offers_the_candidates_that_match_what_follows() {
        let (e, mut rx) = start().await;
        e.set_candidates(&[
            Candidate::new("auth/web", "session"),
            Candidate::new("src/auth.rs", "file"),
            Candidate::new("README.md", "file"),
        ])
        .await
        .unwrap();
        e.input("see @au").await.unwrap();
        wait(&mut rx, |_| {
            e.state().popup().is_some_and(|p| {
                p.items.iter().all(|i| i.word.contains("au")) && p.items.len() == 2
            })
        })
        .await;
        e.input("<C-y>").await.unwrap();
        wait(&mut rx, |_| e.state().screen().text()[0] == "see @auth/web").await;
        assert_eq!(e.take_text().await.unwrap(), "see @auth/web ");
    }

    #[tokio::test]
    async fn moving_down_the_menu_picks_the_candidate_moved_to() {
        let (e, mut rx) = start().await;
        e.set_candidates(&[
            Candidate::new("auth/web", "session"),
            Candidate::new("src/auth.rs", "file"),
        ])
        .await
        .unwrap();
        e.input("@au").await.unwrap();
        wait(&mut rx, |_| {
            e.state().popup().is_some_and(|p| p.items.len() == 2)
        })
        .await;
        e.input("<C-n>").await.unwrap();
        wait(&mut rx, |_| {
            e.state().popup().is_some_and(|p| p.selected == Some(1))
        })
        .await;
        e.input("<C-y>").await.unwrap();
        wait(&mut rx, |_| e.state().popup().is_none()).await;
        assert_eq!(e.take_text().await.unwrap(), "@src/auth.rs ");
    }

    #[tokio::test]
    async fn an_at_sign_inside_a_word_offers_nothing() {
        let (e, mut rx) = start().await;
        e.set_candidates(&[Candidate::new("auth/web", "session")])
            .await
            .unwrap();
        e.input("mail@au").await.unwrap();
        wait(&mut rx, |_| e.state().screen().text()[0] == "mail@au").await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(e.state().popup().is_none());
    }
    #[tokio::test]
    async fn typed_text_with_angle_brackets_is_typed_as_it_is() {
        let (e, mut rx) = start().await;
        e.type_text("a<CR>b").await.unwrap();
        wait(&mut rx, |_| e.state().screen().text()[0] == "a<CR>b").await;
    }

    #[tokio::test]
    async fn a_paste_in_normal_mode_is_inserted_as_text() {
        let (e, mut rx) = start().await;
        e.input("<Esc>").await.unwrap();
        wait(&mut rx, |_| e.state().mode().starts_with("normal")).await;
        e.paste("dd").await.unwrap();
        wait(&mut rx, |_| e.state().screen().text()[0] == "dd").await;
    }

    #[tokio::test]
    async fn taking_the_text_puts_the_cursor_back_at_the_start() {
        // With the cursor free to stay past the end of a line, it would
        // stay where the text ended.
        let (tx, mut rx) = mpsc::unbounded_channel();
        let e = Embedded::spawn(
            &crate::program(),
            &["--clean", "--cmd", "set virtualedit=all"],
            30,
            4,
            move |ev| {
                let _ = tx.send(ev);
            },
        )
        .await
        .expect("nvim on PATH");
        e.type_text("Hello").await.unwrap();
        wait(&mut rx, |_| {
            e.state().screen().cursor.is_some_and(|c| c.col == 5)
        })
        .await;
        assert_eq!(e.take_text().await.unwrap(), "Hello");
        wait(&mut rx, |_| {
            e.state().screen().cursor.is_some_and(|c| c.col == 0)
        })
        .await;
    }

    #[tokio::test]
    async fn the_prompt_s_text_is_taken_even_from_another_buffer() {
        let (e, mut rx) = start().await;
        e.type_text("mine").await.unwrap();
        e.input("<Esc>:enew<CR>iother<Esc>").await.unwrap();
        wait(&mut rx, |_| e.state().screen().text()[0] == "other").await;
        assert_eq!(e.take_text().await.unwrap(), "mine");
        until(&mut rx, || e.state().screen().text()[0].is_empty()).await;
    }

    #[tokio::test]
    async fn the_events_end_when_neovim_quits() {
        let (e, mut rx) = start().await;
        e.input("<Esc>:qa!<CR>").await.ok();
        tokio::time::timeout(Duration::from_secs(10), async {
            while rx.recv().await.is_some() {}
        })
        .await
        .expect("the events went on after Neovim quit");
    }

    #[tokio::test]
    async fn taken_back_lines_fill_an_empty_prompt() {
        let (e, _rx) = start().await;
        e.append(&["one".into(), "two".into()]).await.unwrap();
        assert_eq!(e.text().await.unwrap(), "one\ntwo");
    }

    #[tokio::test]
    async fn taken_back_lines_follow_what_is_written() {
        let (e, mut rx) = start().await;
        e.type_text("draft").await.unwrap();
        wait(&mut rx, |_| e.state().screen().text()[0] == "draft").await;
        e.append(&["one".into()]).await.unwrap();
        assert_eq!(e.text().await.unwrap(), "draft\none");
    }

    #[tokio::test]
    async fn taken_back_lines_leave_the_cursor_where_it_is_in_what_is_written() {
        let (e, mut rx) = start().await;
        e.type_text("draft").await.unwrap();
        wait(&mut rx, |_| e.state().screen().text()[0] == "draft").await;
        e.input("<Esc>0l").await.unwrap();
        until(&mut rx, || e.state().mode() == "normal").await;
        e.append(&["one".into()]).await.unwrap();
        assert_eq!(e.cursor().await.unwrap(), (1, 1));
    }

    #[tokio::test]
    async fn taken_back_lines_in_an_empty_prompt_put_the_cursor_at_their_end() {
        let (e, _rx) = start().await;
        e.append(&["one".into(), "two".into()]).await.unwrap();
        assert_eq!(e.cursor().await.unwrap(), (2, 3));
    }

    #[tokio::test]
    async fn the_cursor_is_put_where_asked_within_the_text() {
        let (e, _rx) = start().await;
        e.append(&["one".into(), "two".into()]).await.unwrap();
        e.set_cursor((1, 2)).await.unwrap();
        assert_eq!(e.cursor().await.unwrap(), (1, 2));
        // A cursor kept for longer text than the prompt now holds stays in it.
        e.set_cursor((9, 0)).await.unwrap();
        assert_eq!(e.cursor().await.unwrap(), (2, 0));
    }

    #[tokio::test]
    async fn a_slash_offers_the_commands_beginning_with_what_follows_before_those_holding_it() {
        let (e, mut rx) = start().await;
        e.set_commands(&[
            Candidate::new("/compact", "Clear history but keep a summary"),
            Candidate::new("/pr-review", "Review a pull request"),
            Candidate::new("/review", "Review code"),
        ])
        .await
        .unwrap();
        e.input("/re").await.unwrap();
        wait(&mut rx, |_| {
            e.state().popup().is_some_and(|p| {
                p.items.iter().map(|i| i.word.as_str()).collect::<Vec<_>>()
                    == ["/review", "/pr-review"]
            })
        })
        .await;
        e.input("<C-y>").await.unwrap();
        wait(&mut rx, |_| e.state().popup().is_none()).await;
        assert_eq!(e.take_text().await.unwrap(), "/review ");
    }

    #[tokio::test]
    async fn a_codex_skill_is_completed_from_a_slash_or_dollar_as_a_dollar_invocation() {
        for trigger in ["/deploy", "$deploy"] {
            let (e, mut rx) = start().await;
            e.set_commands(&[
                Candidate::new("/review", "Review code"),
                Candidate::new("$deploy", "Deploy the service"),
            ])
            .await
            .unwrap();
            e.input(trigger).await.unwrap();
            wait(&mut rx, |_| {
                e.state()
                    .popup()
                    .is_some_and(|p| p.items.iter().any(|i| i.word == "$deploy"))
            })
            .await;
            e.input("<C-y>").await.unwrap();
            wait(&mut rx, |_| e.state().popup().is_none()).await;
            assert_eq!(e.take_text().await.unwrap(), "$deploy ");
        }
    }

    #[tokio::test]
    async fn a_slash_after_a_space_offers_the_commands_too() {
        let (e, mut rx) = start().await;
        e.set_commands(&[Candidate::new("/review", "Review code")])
            .await
            .unwrap();
        e.input("then /r").await.unwrap();
        wait(&mut rx, |_| {
            e.state().popup().is_some_and(|p| p.items.len() == 1)
        })
        .await;
    }

    #[tokio::test]
    async fn a_slash_inside_a_word_offers_no_command() {
        let (e, mut rx) = start().await;
        e.set_commands(&[Candidate::new("/review", "Review code")])
            .await
            .unwrap();
        e.input("and/r").await.unwrap();
        wait(&mut rx, |_| e.state().screen().text()[0] == "and/r").await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(e.state().popup().is_none());
    }

    #[tokio::test]
    async fn a_slash_tells_the_window_the_commands_are_wanted() {
        let (e, mut rx) = start().await;
        e.input("/").await.unwrap();
        wait(&mut rx, |ev| *ev == Event::CommandsWanted).await;
    }

    #[tokio::test]
    async fn the_menu_shows_what_a_command_does_when_it_is_picked() {
        let (e, mut rx) = start().await;
        e.set_commands(&[Candidate::new("/review", "Review code").with_info("[pr]")])
            .await
            .unwrap();
        e.input("/r<C-n>").await.unwrap();
        wait(&mut rx, |_| {
            e.state()
                .popup()
                .is_some_and(|p| p.items[0].menu == "Review code" && p.items[0].info == "[pr]")
        })
        .await;
    }

    /// What the prompt's language server answers for completion where the
    /// cursor is: each item's label, the text it puts and the byte it puts
    /// it from.
    async fn served(e: &Embedded) -> Value {
        e.exec_lua(
            r#"
            local b = vim.g.adroit_buf
            local params = vim.lsp.util.make_position_params(0, "utf-8")
            local got = vim.lsp.buf_request_sync(b, "textDocument/completion", params, 2000)
            local out = {}
            for _, response in pairs(got or {}) do
              for _, item in ipairs(response.result.items) do
                out[#out + 1] = {
                  item.label,
                  item.textEdit.newText,
                  item.textEdit.range.start.character,
                }
              end
            end
            return out
            "#,
            vec![],
        )
        .await
        .unwrap()
    }

    fn item(label: &str, text: &str, from: i64) -> Value {
        Value::from(vec![
            Value::from(label),
            Value::from(text),
            Value::from(from),
        ])
    }

    #[tokio::test]
    async fn a_completion_plugin_is_offered_what_the_prompt_offers_by_its_language_server() {
        let (e, mut rx) = start().await;
        e.set_candidates(&[
            Candidate::new("auth/web", "session"),
            Candidate::new("src/auth.rs", "file"),
            Candidate::new("README.md", "file"),
        ])
        .await
        .unwrap();
        e.input("see @au").await.unwrap();
        wait(&mut rx, |_| e.state().screen().text()[0] == "see @au").await;
        assert_eq!(
            served(&e).await,
            Value::from(vec![
                item("auth/web", "auth/web ", 5),
                item("src/auth.rs", "src/auth.rs ", 5),
            ])
        );
    }

    #[tokio::test]
    async fn a_command_is_served_in_place_of_the_slash_that_starts_it() {
        let (e, mut rx) = start().await;
        e.set_commands(&[Candidate::new("/review", "Review code")])
            .await
            .unwrap();
        e.input("then /r").await.unwrap();
        wait(&mut rx, |_| e.state().screen().text()[0] == "then /r").await;
        assert_eq!(
            served(&e).await,
            Value::from(vec![item("/review", "/review ", 5)])
        );
    }

    #[tokio::test]
    async fn the_language_server_asks_to_be_asked_after_an_at_sign_a_slash_or_a_dollar() {
        let (e, _rx) = start().await;
        let triggers = Value::from(vec![Value::from("@"), Value::from("/"), Value::from("$")]);
        assert!(
            e.settles(
                "luaeval('vim.lsp.get_clients({ name = \"adroit\", bufnr = vim.g.adroit_buf })[1].server_capabilities.completionProvider.triggerCharacters')",
                triggers
            )
            .await
        );
    }

    fn asks(ev: &Event, action: &str) -> bool {
        *ev == Event::Action(action.to_string())
    }

    #[tokio::test]
    async fn the_prompt_s_keys_ask_the_window_for_adroit_s_actions() {
        for (keys, action) in [
            ("<C-CR>", "prompt.send"),
            ("<Tab>", "prompt.take_suggestion"),
            ("<S-Tab>", "console.send Shift+Tab"),
            ("<C-c>", "console.send Escape"),
            ("<C-p>", "prompt.history_prev"),
            ("<C-n>", "prompt.history_next"),
            ("<Esc><C-CR>", "prompt.send"),
        ] {
            let (e, mut rx) = start().await;
            e.input(keys).await.unwrap();
            wait(&mut rx, |ev| asks(ev, action)).await;
        }
    }

    #[tokio::test]
    async fn the_keys_that_move_through_completions_stay_neovim_s_while_it_shows_them() {
        let (e, mut rx) = start().await;
        e.set_candidates(&[
            Candidate::new("auth/web", "session"),
            Candidate::new("src/auth.rs", "file"),
        ])
        .await
        .unwrap();
        e.input("@au").await.unwrap();
        wait(&mut rx, |_| {
            e.state().popup().is_some_and(|p| p.items.len() == 2)
        })
        .await;
        e.input("<C-n>").await.unwrap();
        wait(&mut rx, |ev| {
            assert!(!matches!(ev, Event::Action(_)), "{ev:?}");
            e.state().popup().is_some_and(|p| p.selected == Some(1))
        })
        .await;
    }

    #[tokio::test]
    async fn a_configuration_can_give_the_prompt_s_keys_to_something_else() {
        let (e, mut rx, _config) = start_configured(
            r#"
            vim.keymap.set("i", "<C-p>", function() vim.g.mine = 1 end)
            vim.keymap.set("i", "<C-s>", "<Cmd>Adroit prompt.send<CR>")
            "#,
        )
        .await;
        e.input("<C-p>").await.unwrap();
        assert!(e.settles("get(g:, 'mine', 0)", Value::from(1)).await);
        e.input("<C-s>").await.unwrap();
        wait(&mut rx, |ev| {
            assert!(!asks(ev, "prompt.history_prev"), "{ev:?}");
            asks(ev, "prompt.send")
        })
        .await;
    }

    #[tokio::test]
    async fn a_configuration_can_leave_completing_to_a_plugin_of_its_own() {
        let (e, mut rx, _config) = start_configured("vim.g.adroit_autocomplete = false").await;
        e.set_candidates(&[Candidate::new("auth/web", "session")])
            .await
            .unwrap();
        e.input("@au").await.unwrap();
        wait(&mut rx, |_| e.state().screen().text()[0] == "@au").await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(e.state().popup().is_none());
        assert_eq!(
            served(&e).await,
            Value::from(vec![item("auth/web", "auth/web ", 1)])
        );
    }

    /// A worktree holding `src/lib.rs`, `src/main.rs`, `README.md` and a
    /// hidden file.
    fn worktree(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("adroit-embed-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        for f in ["src/lib.rs", "src/main.rs", "README.md", ".env"] {
            std::fs::write(dir.join(f), "").unwrap();
        }
        dir.canonicalize().unwrap()
    }

    fn words(e: &Embedded) -> Option<Vec<String>> {
        e.state()
            .popup()
            .map(|p| p.items.iter().map(|i| i.word.clone()).collect())
    }

    async fn paths(
        e: &Embedded,
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<Event>,
        typed: &str,
        candidates: &[Candidate],
    ) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(Event::PathsWanted {
                    id,
                    typed: requested,
                    ..
                }) = rx.recv().await
                    && requested == typed
                {
                    e.set_paths(id, candidates).await.unwrap();
                    break;
                }
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn path_offers_use_host_answers_without_reading_even_an_absolute_local_directory() {
        let (e, mut rx) = start().await;
        let dir = worktree("host-only");
        e.exec("vim.fn.readdir = function() error('local filesystem read') end; vim.fn.isdirectory = function() error('local filesystem read') end", vec![]).await.unwrap();
        let typed = format!("{}/", dir.display());
        e.input(&format!("@{typed}")).await.unwrap();
        paths(
            &e,
            &mut rx,
            &typed,
            &[Candidate::new(format!("{typed}remote-only.rs"), "file")],
        )
        .await;
        wait(&mut rx, |_| {
            words(&e).is_some_and(|words| words == [format!("{typed}remote-only.rs")])
        })
        .await;
    }

    #[tokio::test]
    async fn a_frozen_prompt_keeps_its_text_until_that_exact_send_is_consumed() {
        let (e, mut rx) = start().await;
        e.input("original").await.unwrap();
        wait(&mut rx, |_| e.state().screen().text()[0] == "original").await;
        e.freeze_prompt(true).await.unwrap();
        e.input("forbidden").await.unwrap();
        assert!(e.settles("&modifiable", Value::from(0)).await);
        assert_eq!(e.text().await.unwrap(), "original");
        assert!(!e.consume_text("older").await.unwrap());
        assert_eq!(e.text().await.unwrap(), "original");
        assert!(e.consume_text("original").await.unwrap());
        assert_eq!(e.text().await.unwrap(), "");
        assert!(e.settles("&modifiable", Value::from(0)).await);
        e.put_back("restored\n\n").await.unwrap();
        assert_eq!(e.text().await.unwrap(), "restored\n\n");
        assert_eq!(e.take_text().await.unwrap(), "restored\n\n");
        e.freeze_prompt(false).await.unwrap();
        e.input("new").await.unwrap();
        wait(&mut rx, |_| e.state().screen().text()[0] == "new").await;
    }

    #[tokio::test]
    async fn an_at_sign_offers_sessions_before_the_host_s_directory_entries() {
        let (e, mut rx) = start().await;
        e.set_candidates(&[Candidate::new("fix", "session")])
            .await
            .unwrap();
        e.set_root(Some(&worktree("top"))).await.unwrap();
        e.input("@").await.unwrap();
        paths(
            &e,
            &mut rx,
            "",
            &[
                Candidate::new("src/", "dir"),
                Candidate::new("README.md", "file"),
            ],
        )
        .await;
        wait(&mut rx, |_| {
            words(&e).is_some_and(|w| w == ["fix", "src/", "README.md"])
        })
        .await;
    }

    #[tokio::test]
    async fn a_directory_s_entries_are_offered_after_its_slash() {
        let (e, mut rx) = start().await;
        e.set_root(Some(&worktree("inside"))).await.unwrap();
        e.input("@src/m").await.unwrap();
        paths(
            &e,
            &mut rx,
            "src/m",
            &[Candidate::new("src/main.rs", "file")],
        )
        .await;
        wait(&mut rx, |_| words(&e).is_some_and(|w| w == ["src/main.rs"])).await;
        e.input("<C-y>").await.unwrap();
        wait(&mut rx, |_| e.state().popup().is_none()).await;
        assert_eq!(e.take_text().await.unwrap(), "@src/main.rs ");
    }

    #[tokio::test]
    async fn picking_a_directory_goes_on_to_offer_what_is_in_it() {
        let (e, mut rx) = start().await;
        e.set_root(Some(&worktree("walk"))).await.unwrap();
        e.input("@sr").await.unwrap();
        paths(&e, &mut rx, "sr", &[Candidate::new("src/", "dir")]).await;
        wait(&mut rx, |_| words(&e).is_some_and(|w| w == ["src/"])).await;
        e.input("<C-y>").await.unwrap();
        paths(
            &e,
            &mut rx,
            "src/",
            &[
                Candidate::new("src/lib.rs", "file"),
                Candidate::new("src/main.rs", "file"),
            ],
        )
        .await;
        wait(&mut rx, |_| {
            words(&e).is_some_and(|w| w == ["src/lib.rs", "src/main.rs"])
        })
        .await;
        assert_eq!(e.text().await.unwrap(), "@src/");
    }

    #[tokio::test]
    async fn an_at_sign_and_a_slash_walk_from_the_root_of_the_disk() {
        let (e, mut rx) = start().await;
        let dir = worktree("absolute");
        e.input(&format!("@{}/sr", dir.display())).await.unwrap();
        let want = format!("{}/src/", dir.display());
        paths(
            &e,
            &mut rx,
            &format!("{}/sr", dir.display()),
            &[Candidate::new(want.clone(), "dir")],
        )
        .await;
        wait(&mut rx, |_| words(&e).is_some_and(|w| w == [want.clone()])).await;
    }

    #[tokio::test]
    async fn a_tracked_file_deep_in_the_worktree_is_found_by_part_of_its_path() {
        let (e, mut rx) = start().await;
        e.set_candidates(&[
            Candidate::new("README.md", "file"),
            Candidate::new("src/main.rs", "file"),
        ])
        .await
        .unwrap();
        e.set_root(Some(&worktree("tracked"))).await.unwrap();
        e.input("@ma").await.unwrap();
        paths(&e, &mut rx, "ma", &[]).await;
        wait(&mut rx, |_| words(&e).is_some_and(|w| w == ["src/main.rs"])).await;
        e.input("<BS><BS>R").await.unwrap();
        paths(&e, &mut rx, "R", &[Candidate::new("README.md", "file")]).await;
        // The entry and the tracked file are one.
        wait(&mut rx, |_| {
            words(&e).is_some_and(|w| w == ["README.md", "src/main.rs"])
        })
        .await;
    }

    #[tokio::test]
    async fn a_bang_first_tells_the_window_the_prompt_is_a_shell_line_until_it_goes() {
        let (e, mut rx) = start().await;
        e.input("!ls").await.unwrap();
        wait(&mut rx, |ev| *ev == Event::Shell(true)).await;
        e.input("<Home><Del>").await.unwrap();
        wait(&mut rx, |ev| *ev == Event::Shell(false)).await;
        e.paste("!pwd").await.unwrap();
        wait(&mut rx, |ev| *ev == Event::Shell(true)).await;
        e.take_text().await.unwrap();
        wait(&mut rx, |ev| *ev == Event::Shell(false)).await;
    }

    #[tokio::test]
    async fn a_request_given_up_before_its_answer_leaves_the_connection_alive() {
        let (e, mut rx) = start().await;
        // `f` waits for the character to find, and holds back what follows.
        e.input("<Esc>f").await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        let given_up = tokio::time::timeout(Duration::from_millis(300), e.resize(40, 5)).await;
        assert!(given_up.is_err(), "held back by `f`");
        e.input("x").await.unwrap();
        let answered = tokio::time::timeout(Duration::from_secs(2), e.input("ihello")).await;
        assert!(answered.is_ok(), "a later request is answered");
        wait(&mut rx, |_| e.state().screen().text()[0] == "hello").await;
    }
}

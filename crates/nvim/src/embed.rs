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
#[derive(Debug, Clone, PartialEq, Eq)]
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
}

/// Turns the user's Neovim into a prompt editor: the frame around the text is
/// hidden, the buffer is a scratch buffer of its own filetype, and every text
/// change reports how many screen lines the text needs. Runs once startup has
/// finished, after the user's configuration, and holds what it sets against
/// any later change.
pub(crate) const SETUP: &str = concat!(
    include_str!("../lua/hold.lua"),
    include_str!("../lua/config.lua"),
    include_str!("../lua/prompt.lua")
);

/// What the prompt is given before the user's configuration, which may change
/// any of it.
pub(crate) const PROMPT_DEFAULTS: &str = include_str!("../lua/prompt_defaults.lua");

const TAKE_TEXT: &str = include_str!("../lua/take_text.lua");

const APPEND: &str = include_str!("../lua/append.lua");

const TEXT: &str = include_str!("../lua/text.lua");

const CURSOR: &str = include_str!("../lua/cursor.lua");

const SET_CURSOR: &str = include_str!("../lua/set_cursor.lua");

#[derive(Clone)]
struct Listener {
    state: Arc<Mutex<UiState>>,
    events: Arc<dyn Fn(Event) + Send + Sync>,
}

#[async_trait::async_trait]
impl Handler for Listener {
    type Writer = Writer;

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
    /// Lua run before the user's configuration, which may change any of it.
    pub(crate) defaults: &'static str,
    /// Lua run once startup has finished, given the window's channel, which
    /// holds what the window relies on.
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

/// A Neovim process attached as this window's UI.
pub struct Embedded {
    nvim: Neovim<Writer>,
    state: Arc<Mutex<UiState>>,
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
            defaults: PROMPT_DEFAULTS,
            setup: SETUP,
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
            .args(["--cmd", &format!("lua {}", launch.defaults)])
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
        nvim.exec_lua(launch.setup, vec![Value::from(channel)])
            .await?;
        Ok(Self {
            nvim,
            state,
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
        self.state().forget_message();
        let keys = keys.to_string();
        self.ask(move |n| async move { n.input(&keys).await })
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
        self.state().forget_message();
        let text = text.to_string();
        self.ask(move |n| async move { n.paste(&text, false, -1).await })
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
        self.set_var("adroit_root", Value::from(root)).await
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
        let e = Embedded::spawn("nvim", &["--clean"], 30, 4, move |ev| {
            let _ = tx.send(ev);
        })
        .await
        .expect("nvim on PATH");
        (e, rx)
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
        let e = Embedded::spawn("nvim", &args, 30, 4, move |ev| {
            let _ = tx.send(ev);
        })
        .await
        .expect("nvim on PATH");
        (e, rx, config)
    }

    #[tokio::test]
    async fn a_configuration_saved_again_is_read_again_and_still_cannot_change_what_the_window_relies_on()
     {
        let (e, mut rx, config) = start_configured("vim.g.read = 1").await;
        assert!(e.settles("g:read", Value::from(1)).await);
        e.type_text("kept").await.unwrap();
        wait(&mut rx, |_| e.state().screen().text()[0] == "kept").await;
        config.write("vim.g.read = 2\nvim.o.cursorline = true\nvim.o.laststatus = 2");
        assert!(e.settles("g:read", Value::from(2)).await);
        assert!(
            e.settles(
                "[&cursorline, &laststatus]",
                Value::from(vec![Value::from(1), Value::from(0)])
            )
            .await
        );
        assert_eq!(e.text().await.unwrap(), "kept");
    }

    #[tokio::test]
    async fn a_lua_module_of_the_configuration_is_read_anew_with_it() {
        // The folder is on 'runtimepath' as Neovim's own configuration folder is.
        let init = "vim.opt.rtp:prepend(vim.fs.dirname(vim.env.MYVIMRC))\n\
                    vim.g.read = require('mine').n";
        let config = UserConfig::new("");
        let lua = config.init().with_file_name("lua");
        std::fs::create_dir_all(&lua).unwrap();
        std::fs::write(lua.join("mine.lua"), "return { n = 1 }").unwrap();
        config.write(init);
        let args = config.args();
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let e = Embedded::spawn("nvim", &args, 30, 4, |_| {}).await.unwrap();
        assert!(e.settles("g:read", Value::from(1)).await);
        std::fs::write(lua.join("mine.lua"), "return { n = 2 }").unwrap();
        assert!(e.settles("g:read", Value::from(2)).await);
    }

    #[tokio::test]
    async fn a_lua_module_in_a_folder_made_after_starting_is_read_anew_too() {
        let (e, _rx, config) = start_configured(
            "vim.opt.rtp:prepend(vim.fs.dirname(vim.env.MYVIMRC))\n\
             local ok, mine = pcall(require, 'deep.mine')\n\
             vim.g.read = ok and mine.n or 0",
        )
        .await;
        assert!(e.settles("g:read", Value::from(0)).await);
        let deep = config.init().with_file_name("lua").join("deep");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("mine.lua"), "return { n = 1 }").unwrap();
        assert!(e.settles("g:read", Value::from(1)).await);
        std::fs::write(deep.join("mine.lua"), "return { n = 2 }").unwrap();
        assert!(e.settles("g:read", Value::from(2)).await);
    }

    #[tokio::test]
    async fn a_configuration_that_fails_when_read_again_leaves_the_prompt_working() {
        let (e, mut rx, config) = start_configured("vim.g.read = 1").await;
        assert!(e.settles("g:read", Value::from(1)).await);
        config.write("error('broken')");
        // The failure is told, and the prompt still takes text.
        wait(&mut rx, |_| {
            e.state().message().is_some_and(|m| m.contains("broken"))
        })
        .await;
        e.type_text("still").await.unwrap();
        wait(&mut rx, |_| e.state().screen().text()[0] == "still").await;
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
            vim.api.nvim_create_autocmd("FileType", {
              pattern = "adroit-prompt",
              callback = function() vim.wo.wrap = false end,
            })
            "#,
        )
        .await;
        assert!(e.settles("&number", Value::from(1)).await);
        assert!(e.settles("&cursorline", Value::from(1)).await);
        assert!(e.settles("&wrap", Value::from(0)).await);
    }

    #[tokio::test]
    async fn the_user_s_configuration_cannot_change_what_the_window_relies_on() {
        let (e, _rx, _config) = start_configured(
            r#"
            vim.o.laststatus = 2
            vim.o.showtabline = 2
            vim.o.cmdheight = 1
            vim.o.completeopt = "menu"
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
        let held = "[&laststatus, &showtabline, &cmdheight, &completeopt, &eventignore, \
                    &winbar, &modifiable, &buftype, &filetype]";
        let want = Value::from(vec![
            Value::from(0),
            Value::from(0),
            Value::from(0),
            Value::from("menuone,noinsert"),
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
        let e = Embedded::spawn("nvim", &["--clean"], 30, 2, move |ev| {
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
            "nvim",
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

    #[tokio::test]
    async fn an_at_sign_offers_the_sessions_then_the_worktree_s_entries_without_hidden_ones() {
        let (e, mut rx) = start().await;
        e.set_candidates(&[Candidate::new("fix", "session")])
            .await
            .unwrap();
        e.set_root(Some(&worktree("top"))).await.unwrap();
        e.input("@").await.unwrap();
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
        wait(&mut rx, |_| words(&e).is_some_and(|w| w == ["src/"])).await;
        e.input("<C-y>").await.unwrap();
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
        wait(&mut rx, |_| words(&e).is_some_and(|w| w == ["src/main.rs"])).await;
        e.input("<BS><BS>R").await.unwrap();
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

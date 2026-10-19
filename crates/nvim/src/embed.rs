use std::sync::{Arc, Mutex};

use nvim_rs::compat::tokio::Compat;
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
/// finished, so the user's configuration is already loaded.
pub const SETUP: &str = r#"
local chan = ...
vim.g.adroit_channel = chan
local function apply()
  local o = vim.o
  o.laststatus = 0
  o.showtabline = 0
  o.ruler = false
  o.showmode = false
  o.cmdheight = 0
  o.shortmess = o.shortmess .. "IcF"
  vim.cmd("silent! enew")
  local b = vim.api.nvim_get_current_buf()
  vim.g.adroit_buf = b
  vim.bo[b].buftype = "nofile"
  vim.bo[b].bufhidden = "hide"
  vim.bo[b].swapfile = false
  vim.bo[b].filetype = "adroit-prompt"
  local w = vim.api.nvim_get_current_win()
  vim.wo[w].number = false
  vim.wo[w].relativenumber = false
  vim.wo[w].signcolumn = "no"
  vim.wo[w].foldcolumn = "0"
  vim.wo[w].statuscolumn = ""
  vim.wo[w].cursorline = false
  vim.wo[w].wrap = true
  vim.wo[w].linebreak = true
  vim.opt_local.fillchars:append({ eob = " " })
  -- `@` at the start of a word offers the sessions and files the window sets
  -- in g:adroit_candidates and the entries of the directory typed, walked from
  -- g:adroit_root; `/` there offers the commands in g:adroit_commands.
  local function word_start(before, pattern)
    local s = before:find(pattern)
    if not s or (s > 1 and not before:sub(s - 1, s - 1):match("%s")) then
      return nil
    end
    return s, before:sub(s + 1)
  end
  local LIMIT = 200
  local function item(c, word)
    return { word = word, abbr = c.word, menu = c.menu, info = c.info }
  end
  local function entries(dir)
    local ok, names = pcall(vim.fn.readdir, dir)
    if not ok then
      return {}
    end
    local out = {}
    for _, name in ipairs(names) do
      -- Hidden entries are left to be typed, as a shell's completion does.
      if name:sub(1, 1) ~= "." then
        out[#out + 1] = { name = name, dir = vim.fn.isdirectory(dir .. "/" .. name) == 1 }
      end
    end
    table.sort(out, function(x, y)
      if x.dir ~= y.dir then
        return x.dir
      end
      return x.name:lower() < y.name:lower()
    end)
    return out
  end
  local function paths(typed, out, seen)
    local cut = typed:match(".*()/")
    local head = cut and typed:sub(1, cut) or ""
    local want = typed:sub(#head + 1):lower()
    local root = vim.g.adroit_root or ""
    local dir
    if head:sub(1, 1) == "/" then
      dir = head
    elseif root ~= "" then
      dir = root .. "/" .. head
    else
      return
    end
    local n = 0
    for _, e in ipairs(entries(dir)) do
      if e.name:sub(1, #want):lower() == want then
        local path = head .. e.name .. (e.dir and "/" or "")
        seen[path] = true
        -- A directory takes no space after it, so completing goes on inside.
        out[#out + 1] = { word = e.dir and path or path .. " ", abbr = path, menu = e.dir and "dir" or "file" }
        n = n + 1
        if n >= LIMIT then
          return
        end
      end
    end
  end
  local function offer_paths(s, typed)
    local want = typed:lower()
    local sessions, files, seen = {}, {}, {}
    for _, c in ipairs(vim.g.adroit_candidates or {}) do
      if c.word:lower():find(want, 1, true) then
        local list = c.menu == "file" and files or sessions
        if #list < LIMIT then
          list[#list + 1] = c
        end
      end
    end
    local out = {}
    for _, c in ipairs(sessions) do
      out[#out + 1] = item(c, c.word .. " ")
    end
    paths(typed, out, seen)
    for _, c in ipairs(files) do
      if not seen[c.word] then
        out[#out + 1] = item(c, c.word .. " ")
      end
    end
    vim.fn.complete(s + 1, out)
  end
  local function offer_commands(s, typed)
    if typed == "" then
      vim.rpcnotify(chan, "adroit_commands_wanted")
    end
    local want = "/" .. typed:lower()
    local first, then_ = {}, {}
    for _, c in ipairs(vim.g.adroit_commands or {}) do
      local name = c.word:lower()
      if name:sub(1, #want) == want then
        first[#first + 1] = item(c, c.word .. " ")
      elseif name:find(want:sub(2), 2, true) then
        then_[#then_ + 1] = item(c, c.word .. " ")
      end
    end
    vim.list_extend(first, then_)
    vim.fn.complete(s, first)
  end
  local function offer()
    local before = vim.api.nvim_get_current_line():sub(1, vim.fn.col(".") - 1)
    local s, typed = word_start(before, "@[^%s@]*$")
    if s then
      return offer_paths(s, typed)
    end
    s, typed = word_start(before, "/[^%s/]*$")
    if s then
      return offer_commands(s, typed)
    end
  end
  vim.o.completeopt = "menuone,noinsert"
  vim.api.nvim_create_autocmd({ "TextChangedI", "TextChangedP" }, { buffer = b, callback = offer })
  -- Told on every change, from wherever it comes: typed, pasted, or set by
  -- the window.
  local shell = false
  vim.api.nvim_buf_attach(b, false, {
    on_lines = function()
      vim.schedule(function()
        if not vim.api.nvim_buf_is_valid(b) then
          return
        end
        local now = (vim.api.nvim_buf_get_lines(b, 0, 1, false)[1] or ""):sub(1, 1) == "!"
        if now ~= shell then
          shell = now
          vim.rpcnotify(chan, "adroit_shell", now)
        end
      end)
    end,
  })
  local function report()
    local ok, h = pcall(vim.api.nvim_win_text_height, 0, {})
    local need = ok and h.all or vim.api.nvim_buf_line_count(0)
    -- A window grown to fit the text keeps the scroll it needed while it was
    -- smaller, hiding the first lines above an empty one.
    if need <= vim.api.nvim_win_get_height(0) and vim.fn.line("w0") > 1 then
      vim.fn.winrestview({ topline = 1 })
    end
    vim.rpcnotify(chan, "adroit_height", need)
  end
  local function view()
    vim.rpcnotify(chan, "adroit_view", vim.fn.line("w0") - 1, vim.api.nvim_buf_line_count(0))
  end
  vim.api.nvim_create_autocmd(
    { "TextChanged", "TextChangedI", "TextChangedP", "VimResized", "BufEnter", "WinResized" },
    { callback = report }
  )
  vim.api.nvim_create_autocmd(
    { "WinScrolled", "TextChanged", "TextChangedI", "BufEnter" },
    { callback = view }
  )
  report()
  vim.cmd("startinsert")
end
if vim.v.vim_did_enter == 1 then
  apply()
else
  vim.api.nvim_create_autocmd("VimEnter", { once = true, callback = apply })
end
"#;

const TAKE_TEXT: &str = r#"
-- The prompt's own buffer, shown again if `:e` or `:help` left it.
local b = vim.g.adroit_buf
if not (b and vim.api.nvim_buf_is_valid(b)) then
  b = vim.api.nvim_get_current_buf()
elseif vim.api.nvim_get_current_buf() ~= b then
  vim.api.nvim_win_set_buf(0, b)
end
local lines = vim.api.nvim_buf_get_lines(b, 0, -1, false)
vim.api.nvim_buf_set_lines(b, 0, -1, false, {})
-- With `virtualedit` letting it stay past the end of a line, the cursor would
-- otherwise wait where the text ended.
vim.api.nvim_win_set_cursor(0, { 1, 0 })
return table.concat(lines, "\n")
"#;

const APPEND: &str = r#"
local lines = ...
-- The prompt's own buffer, shown again if `:e` or `:help` left it.
local b = vim.g.adroit_buf
if not (b and vim.api.nvim_buf_is_valid(b)) then
  b = vim.api.nvim_get_current_buf()
elseif vim.api.nvim_get_current_buf() ~= b then
  vim.api.nvim_win_set_buf(0, b)
end
local cur = vim.api.nvim_buf_get_lines(b, 0, -1, false)
if #cur == 1 and cur[1] == "" then
  vim.api.nvim_buf_set_lines(b, 0, -1, false, lines)
else
  vim.api.nvim_buf_set_lines(b, -1, -1, false, lines)
end
local n = vim.api.nvim_buf_line_count(b)
vim.api.nvim_win_set_cursor(0, { n, #vim.api.nvim_buf_get_lines(b, n - 1, n, false)[1] })
"#;

const TEXT: &str = r#"
local b = vim.g.adroit_buf
if not (b and vim.api.nvim_buf_is_valid(b)) then
  b = 0
end
return table.concat(vim.api.nvim_buf_get_lines(b, 0, -1, false), "\n")
"#;

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
pub const PROMPT_APPNAME: &str = "adroit/prompt/nvim";

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
        Self::spawn_with(program, args, cols, rows, PROMPT_APPNAME, SETUP, events).await
    }

    /// Start `program` as [`Embedded::spawn`] does, under `appname`, made
    /// ready by `setup`, a Lua chunk given the window's channel.
    pub(crate) async fn spawn_with(
        program: &str,
        args: &[&str],
        cols: u16,
        rows: u16,
        appname: &str,
        setup: &str,
        events: impl Fn(Event) + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        let state = Arc::new(Mutex::new(UiState::default()));
        let listener = Listener {
            state: state.clone(),
            events: Arc::new(events),
        };
        let mut cmd = Command::new(program);
        cmd.arg("--embed")
            .env("NVIM_APPNAME", appname)
            .args(["--cmd", "let g:adroit = v:true"])
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
        nvim.exec_lua(setup, vec![Value::from(channel)]).await?;
        Ok(Self {
            nvim,
            state,
            _child: child,
        })
    }

    #[cfg(test)]
    pub(crate) async fn stdpath(&self, kind: &str) -> String {
        let path = self
            .nvim
            .call_function("stdpath", vec![Value::from(kind)])
            .await
            .unwrap();
        path.as_str().unwrap().to_string()
    }

    #[cfg(test)]
    pub(crate) async fn eval(&self, expr: &str) -> Value {
        self.nvim.eval(expr).await.unwrap()
    }

    pub(crate) async fn exec(&self, lua: &str, args: Vec<Value>) -> Result<(), Error> {
        self.nvim.exec_lua(lua, args).await?;
        Ok(())
    }

    pub fn state(&self) -> std::sync::MutexGuard<'_, UiState> {
        self.state.lock().unwrap()
    }

    /// Type keys in Neovim's notation.
    pub async fn input(&self, keys: &str) -> Result<(), Error> {
        self.nvim.input(keys).await?;
        Ok(())
    }

    pub async fn resize(&self, cols: u16, rows: u16) -> Result<(), Error> {
        self.nvim.ui_try_resize(cols as i64, rows as i64).await?;
        Ok(())
    }

    /// Leave whatever mode the prompt is in and start writing after its text.
    pub async fn start_writing(&self) -> Result<(), Error> {
        self.input("<C-\\><C-N>GA").await
    }

    /// Go back to normal mode, from whatever mode the prompt is in.
    pub async fn stop_writing(&self) -> Result<(), Error> {
        self.input("<C-\\><C-N>").await
    }

    /// Type text as if from the keyboard: a `<` in it is a `<`, not the start
    /// of a key name.
    pub async fn type_text(&self, text: &str) -> Result<(), Error> {
        self.input(&text.replace('<', "<lt>")).await
    }

    /// Put text into the prompt as text, whatever mode it is in.
    pub async fn paste(&self, text: &str) -> Result<(), Error> {
        self.nvim.paste(text, false, -1).await?;
        Ok(())
    }

    /// Replace what `@` in the prompt offers.
    pub async fn set_candidates(&self, candidates: &[Candidate]) -> Result<(), Error> {
        self.nvim
            .set_var("adroit_candidates", candidate_list(candidates))
            .await?;
        Ok(())
    }

    /// Replace what `/` in the prompt offers; none offers nothing.
    pub async fn set_commands(&self, commands: &[Candidate]) -> Result<(), Error> {
        self.nvim
            .set_var("adroit_commands", candidate_list(commands))
            .await?;
        Ok(())
    }

    /// The directory a path after `@` is walked from; without one, a path is
    /// walked only from the root of the disk.
    pub async fn set_root(&self, root: Option<&std::path::Path>) -> Result<(), Error> {
        let root = root
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.nvim.set_var("adroit_root", Value::from(root)).await?;
        Ok(())
    }

    /// The prompt's text.
    pub async fn text(&self) -> Result<String, Error> {
        Ok(self
            .nvim
            .exec_lua(TEXT, vec![])
            .await?
            .as_str()
            .unwrap_or_default()
            .to_string())
    }

    /// Add lines after what the prompt holds, or in its place when it is
    /// empty, with the cursor at their end.
    pub async fn append(&self, lines: &[String]) -> Result<(), Error> {
        let list = lines.iter().map(|l| Value::from(l.as_str())).collect();
        self.nvim.exec_lua(APPEND, vec![Value::Array(list)]).await?;
        Ok(())
    }

    /// The prompt's text, leaving the prompt empty for the next one.
    pub async fn take_text(&self) -> Result<String, Error> {
        Ok(self
            .nvim
            .exec_lua(TAKE_TEXT, vec![])
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

    #[tokio::test]
    async fn it_keeps_its_configuration_and_state_apart_from_the_everyday_neovim() {
        let (e, _rx) = start().await;
        for kind in ["config", "data", "state"] {
            let path = e.stdpath(kind).await;
            assert!(path.ends_with("adroit/prompt/nvim"), "{kind}: {path}");
        }
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
}

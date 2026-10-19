//! Neovim as a reader of what a console shows: the text in a buffer that
//! cannot be changed, its colours kept, for moving, searching, selecting and
//! copying with the user's own Vim keys.

use rmpv::Value;

use crate::embed::{Embedded, Error, Event};

/// The name the console's reader runs under, so its configuration, plugins
/// and state are found in folders of their own, apart from the everyday
/// Neovim's and the prompt's.
pub const CONSOLE_APPNAME: &str = "adroit/console/nvim";

/// Turns the user's Neovim into the console's reader: no frame around the
/// text, lines kept unwrapped as the terminal shows them, and a yank told to
/// the window, which copies it.
pub const READER: &str = r#"
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
  -- The window tells of a copy itself.
  o.report = 99999
  vim.cmd("silent! enew")
  local b = vim.api.nvim_get_current_buf()
  vim.g.adroit_buf = b
  vim.bo[b].buftype = "nofile"
  vim.bo[b].bufhidden = "hide"
  vim.bo[b].swapfile = false
  vim.bo[b].filetype = "adroit-console"
  vim.bo[b].modifiable = false
  local w = vim.api.nvim_get_current_win()
  vim.wo[w].number = false
  vim.wo[w].relativenumber = false
  vim.wo[w].signcolumn = "no"
  vim.wo[w].foldcolumn = "0"
  vim.wo[w].statuscolumn = ""
  vim.wo[w].wrap = false
  vim.wo[w].list = false
  vim.opt_local.fillchars:append({ eob = " " })
  -- The reader is for moving, searching and copying. Commands, Ex mode and
  -- quitting would leave it broken or gone, and windows and tabs arranged in
  -- it would stay. `<C-w>` takes the key after it too, or `<C-w>q` would
  -- start recording.
  for _, lhs in ipairs({ ":", "Q", "gQ", "ZZ", "ZQ" }) do
    vim.keymap.set({ "n", "x" }, lhs, "<Nop>")
  end
  vim.keymap.set({ "n", "x" }, "<C-w>", function()
    vim.fn.getcharstr()
  end)
  -- Keys are not the only way: a mapping or a plugin of the user's may open
  -- a window, as `K` opens help.
  vim.api.nvim_create_autocmd({ "WinNew", "TabNew" }, {
    callback = function()
      vim.schedule(function()
        if vim.api.nvim_win_is_valid(w) then
          vim.api.nvim_set_current_win(w)
        end
        vim.cmd("silent! tabonly! | silent! only!")
        if vim.api.nvim_buf_is_valid(b) then
          vim.api.nvim_win_set_buf(0, b)
        end
        w = vim.api.nvim_get_current_win()
      end)
    end,
  })
  -- Where the text is scrolled to, so the window can put images over it.
  local function view()
    local v = vim.fn.winsaveview()
    vim.rpcnotify(chan, "adroit_scrolled", v.topline - 1, v.leftcol)
  end
  vim.g.adroit_view = view
  vim.api.nvim_create_autocmd({ "WinScrolled", "BufEnter", "VimResized" }, { callback = view })
  vim.api.nvim_create_autocmd("TextYankPost", {
    buffer = b,
    callback = function()
      local e = vim.v.event
      if e.operator == "y" and (e.regname == "" or e.regname == "+" or e.regname == "*") then
        local text = table.concat(e.regcontents, "\n")
        if e.regtype == "V" then
          text = text .. "\n"
        end
        vim.rpcnotify(chan, "adroit_yank", text, #e.regcontents)
      end
    end,
  })
end
if vim.v.vim_did_enter == 1 then
  apply()
else
  vim.api.nvim_create_autocmd("VimEnter", { once = true, callback = apply })
end
"#;

const SHOW: &str = r#"
local lines, marks, cur, follow, background = ...
local b = vim.g.adroit_buf
if not (b and vim.api.nvim_buf_is_valid(b)) then
  return
end
if vim.api.nvim_get_current_buf() ~= b then
  vim.api.nvim_win_set_buf(0, b)
end
local ns = vim.api.nvim_create_namespace("adroit_console")
local was = vim.api.nvim_win_get_cursor(0)
local view = vim.fn.winsaveview()
vim.bo[b].modifiable = true
vim.api.nvim_buf_set_lines(b, 0, -1, false, lines)
vim.bo[b].modifiable = false
vim.api.nvim_buf_clear_namespace(b, ns, 0, -1)
vim.api.nvim_set_hl(0, "Normal", { bg = background, fg = vim.api.nvim_get_hl(0, { name = "Normal" }).fg })
for _, m in ipairs(marks) do
  local group = m[4]
  if vim.fn.hlexists(group) == 0 then
    vim.api.nvim_set_hl(0, group, {
      fg = m[5],
      bg = m[6] ~= "" and m[6] or nil,
      bold = m[7],
      italic = m[8],
      underline = m[9],
    })
  end
  pcall(vim.api.nvim_buf_set_extmark, b, ns, m[1], m[2], { end_col = m[3], hl_group = group })
end
if follow then
  vim.api.nvim_win_set_cursor(0, { cur[1] + 1, cur[2] })
else
  local n = vim.api.nvim_buf_line_count(b)
  vim.fn.winrestview(view)
  vim.api.nvim_win_set_cursor(0, { math.min(was[1], n), was[2] })
end
if vim.g.adroit_view then
  vim.g.adroit_view()
end
"#;

/// A stretch of a line drawn in one colour: the line counted from 0, the
/// bytes it covers, and how it is drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    pub line: usize,
    pub start: usize,
    pub end: usize,
    pub fg: String,
    pub bg: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl Mark {
    /// The highlight group the mark is drawn with, one for each look.
    fn group(&self) -> String {
        format!(
            "AdroitConsole_{}_{}_{}{}{}",
            self.fg.trim_start_matches('#'),
            self.bg.as_deref().unwrap_or("none").trim_start_matches('#'),
            u8::from(self.bold),
            u8::from(self.italic),
            u8::from(self.underline)
        )
    }
}

impl Embedded {
    /// Start `program` as a reader of a console `cols` × `rows`.
    pub async fn spawn_reader(
        program: &str,
        args: &[&str],
        cols: u16,
        rows: u16,
        events: impl Fn(Event) + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        Self::spawn_with(program, args, cols, rows, CONSOLE_APPNAME, READER, events).await
    }

    /// Show `lines` with their `marks` on `background`. With `follow`, the
    /// cursor goes to `cursor`, a line and a byte counted from 0; otherwise
    /// it stays where the user left it.
    pub async fn show(
        &self,
        lines: &[String],
        marks: &[Mark],
        cursor: (usize, usize),
        follow: bool,
        background: &str,
    ) -> Result<(), Error> {
        let lines = Value::Array(lines.iter().map(|l| Value::from(l.as_str())).collect());
        let marks = Value::Array(
            marks
                .iter()
                .map(|m| {
                    Value::Array(vec![
                        Value::from(m.line as u64),
                        Value::from(m.start as u64),
                        Value::from(m.end as u64),
                        Value::from(m.group()),
                        Value::from(m.fg.as_str()),
                        Value::from(m.bg.as_deref().unwrap_or("")),
                        Value::from(m.bold),
                        Value::from(m.italic),
                        Value::from(m.underline),
                    ])
                })
                .collect(),
        );
        let cursor = Value::Array(vec![
            Value::from(cursor.0 as u64),
            Value::from(cursor.1 as u64),
        ]);
        self.exec(
            SHOW,
            vec![
                lines,
                marks,
                cursor,
                Value::from(follow),
                Value::from(background),
            ],
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::sync::mpsc;

    use super::*;

    async fn start() -> (Embedded, mpsc::UnboundedReceiver<Event>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let e = Embedded::spawn_reader("nvim", &["--clean"], 30, 4, move |ev| {
            let _ = tx.send(ev);
        })
        .await
        .expect("nvim on PATH");
        (e, rx)
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

    fn lines(text: &[&str]) -> Vec<String> {
        text.iter().map(|l| l.to_string()).collect()
    }

    #[tokio::test]
    async fn it_keeps_its_configuration_and_state_apart_from_the_prompt_s() {
        let (e, _rx) = start().await;
        for kind in ["config", "data", "state"] {
            let path = e.stdpath(kind).await;
            assert!(path.ends_with("adroit/console/nvim"), "{kind}: {path}");
        }
    }

    /// Wait, at most two seconds, for `expr` to be `want`.
    async fn settles(e: &Embedded, expr: &str, want: Value) -> bool {
        tokio::time::timeout(Duration::from_secs(2), async {
            while e.eval(expr).await != want {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .is_ok()
    }

    /// Windows, tabs, and whether the console's buffer is the one shown.
    const LAYOUT: &str = "[winnr('$'), tabpagenr('$'), bufnr() == g:adroit_buf]";

    fn alone() -> Value {
        Value::from(vec![Value::from(1), Value::from(1), Value::from(1)])
    }

    #[tokio::test]
    async fn a_window_or_a_tab_opened_in_it_closes_at_once() {
        let (e, _rx) = start().await;
        e.show(&lines(&["one", "two"]), &[], (0, 0), true, "#101010")
            .await
            .unwrap();
        // As a plugin or a mapping of the user's would open them.
        for cmd in ["split", "vnew", "tabnew", "help"] {
            e.exec(&format!("vim.cmd('{cmd}')"), vec![]).await.unwrap();
            assert!(
                settles(&e, LAYOUT, alone()).await,
                "{cmd}: {}",
                e.eval(LAYOUT).await
            );
        }
    }

    #[tokio::test]
    async fn its_keys_neither_arrange_windows_nor_run_commands_nor_quit() {
        let (e, _rx) = start().await;
        e.show(&lines(&["one", "two"]), &[], (0, 0), true, "#101010")
            .await
            .unwrap();
        let keys = [
            "<C-w>s",
            "<C-w>v",
            "<C-w>q",
            ":let g:ran = 1<CR>",
            "Vj:<C-u>let g:ran = 1<CR>",
            "gQlet g:ran = 1<CR>visual<CR>",
            "Qlet g:ran = 1<CR>visual<CR>",
            "ZZ",
            "ZQ",
        ];
        // Typed and taken before the call returns, as `input` is not; a key
        // that fails, as `s` does on text that cannot change, may fail it.
        let typed = "vim.api.nvim_feedkeys(vim.keycode(...), 'xt', false)";
        for keys in keys {
            let taken =
                tokio::time::timeout(Duration::from_secs(2), e.exec(typed, vec![keys.into()]))
                    .await;
            assert!(taken.is_ok(), "{keys}: still waiting for keys");
            assert_eq!(e.eval(LAYOUT).await, alone(), "{keys}");
            assert_eq!(e.eval("exists('g:ran')").await, Value::from(0), "{keys}");
            assert_eq!(e.eval("reg_recording()").await, Value::from(""), "{keys}");
        }
        // Searching stays.
        e.exec(typed, vec!["gg/two<CR>".into()]).await.unwrap();
        assert_eq!(e.eval("line('.')").await, Value::from(2));
    }

    #[tokio::test]
    async fn the_text_is_shown_with_the_cursor_where_asked_and_cannot_be_changed() {
        let (e, mut rx) = start().await;
        e.show(
            &lines(&["one", "two", "three"]),
            &[],
            (2, 0),
            true,
            "#101010",
        )
        .await
        .unwrap();
        wait(&mut rx, |_| e.state().screen().text()[2] == "three").await;
        e.input("xdd").await.unwrap();
        e.input("k").await.unwrap();
        wait(&mut rx, |_| {
            e.state().screen().cursor.is_some_and(|c| c.row == 1)
        })
        .await;
        assert_eq!(e.state().screen().text()[..3], ["one", "two", "three"]);
    }

    #[tokio::test]
    async fn new_text_keeps_the_cursor_where_the_user_left_it() {
        let (e, mut rx) = start().await;
        e.show(&lines(&["a", "b", "c"]), &[], (2, 0), true, "#101010")
            .await
            .unwrap();
        e.input("gg").await.unwrap();
        e.show(&lines(&["a", "b", "c", "d"]), &[], (3, 0), false, "#101010")
            .await
            .unwrap();
        wait(&mut rx, |_| e.state().screen().text()[3] == "d").await;
        assert_eq!(e.state().screen().cursor.map(|c| c.row), Some(0));
    }

    #[tokio::test]
    async fn a_yank_is_told_with_its_text() {
        let (e, mut rx) = start().await;
        e.show(
            &lines(&["hello world", "next"]),
            &[],
            (0, 0),
            true,
            "#101010",
        )
        .await
        .unwrap();
        e.input("wyiw").await.unwrap();
        wait(&mut rx, |ev| {
            *ev == Event::Yank {
                text: "world".into(),
                lines: 1,
            }
        })
        .await;
        e.input("Vjy").await.unwrap();
        wait(&mut rx, |ev| {
            *ev == Event::Yank {
                text: "hello world\nnext\n".into(),
                lines: 2,
            }
        })
        .await;
    }

    #[tokio::test]
    async fn where_the_text_is_scrolled_to_is_told() {
        let (e, mut rx) = start().await;
        let ten: Vec<String> = (1..=10).map(|n| n.to_string()).collect();
        e.show(&ten, &[], (9, 0), true, "#101010").await.unwrap();
        // Four rows end at the tenth line.
        wait(&mut rx, |ev| *ev == Event::Scrolled { top: 6, left: 0 }).await;
        e.input("gg").await.unwrap();
        wait(&mut rx, |ev| *ev == Event::Scrolled { top: 0, left: 0 }).await;
    }

    #[test]
    fn each_look_has_a_group_of_its_own() {
        let m = |fg: &str, bold| Mark {
            line: 0,
            start: 0,
            end: 1,
            fg: fg.into(),
            bg: None,
            bold,
            italic: false,
            underline: false,
        };
        assert_ne!(m("#ff0000", false).group(), m("#ff0000", true).group());
        assert_ne!(m("#ff0000", false).group(), m("#00ff00", false).group());
        assert_eq!(m("#ff0000", false).group(), m("#ff0000", false).group());
    }
}

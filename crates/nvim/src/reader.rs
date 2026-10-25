//! Neovim as a reader of what a console shows: the text in a buffer that
//! cannot be changed, its colours kept, for moving, searching, selecting and
//! copying with the user's own Vim keys.

use rmpv::Value;

use crate::embed::{Embedded, Error, Event, Launch};

/// The name the console's reader runs under, so its configuration, plugins
/// and state are found in folders of their own, apart from the everyday
/// Neovim's and the prompt's.
pub(crate) const CONSOLE_APPNAME: &str = "adroit/console/nvim";

/// Turns the user's Neovim into the console's reader: no frame around the
/// text, lines kept unwrapped as the terminal shows them, and a yank told to
/// the window, which copies it. Runs after the user's configuration and holds
/// what it sets against any later change.
pub(crate) const READER: &str = concat!(
    include_str!("../lua/hold.lua"),
    include_str!("../lua/reader.lua")
);

/// What the reader is given before the user's configuration, which may change
/// any of it.
pub(crate) const READER_DEFAULTS: &str = include_str!("../lua/reader_defaults.lua");

const SHOW: &str = include_str!("../lua/show.lua");

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
        let launch = Launch {
            appname: CONSOLE_APPNAME,
            defaults: READER_DEFAULTS,
            setup: READER,
        };
        Self::spawn_with(program, args, cols, rows, launch, events).await
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

    async fn start_configured(init: &str) -> (Embedded, tempfile::TempPath) {
        let init = crate::embed::user_init(init);
        let path = init.to_str().unwrap();
        let e = Embedded::spawn_reader("nvim", &["--clean", "-u", path], 30, 4, |_| {})
            .await
            .expect("nvim on PATH");
        (e, init)
    }

    #[tokio::test]
    async fn the_user_s_configuration_changes_what_adroit_only_suggests() {
        let (e, _init) = start_configured(
            r#"
            vim.o.report = 2
            vim.o.showmode = true
            "#,
        )
        .await;
        assert!(
            e.settles(
                "[&report, &showmode]",
                Value::from(vec![Value::from(2), Value::from(1)])
            )
            .await
        );
    }

    #[tokio::test]
    async fn the_user_s_configuration_cannot_change_what_the_window_relies_on() {
        let (e, _init) = start_configured(
            r#"
            vim.o.number = true
            vim.o.laststatus = 2
            vim.o.foldenable = true
            vim.o.foldmethod = "indent"
            vim.o.conceallevel = 2
            vim.o.winbar = "bar"
            vim.keymap.set("n", ":", ":")
            vim.api.nvim_create_autocmd("FileType", {
              pattern = "adroit-console",
              callback = function()
                vim.wo.wrap = true
                vim.bo.modifiable = true
                vim.keymap.set("n", ":", ":", { buffer = true })
              end,
            })
            "#,
        )
        .await;
        let held = "[&number, &laststatus, &wrap, &modifiable, &foldenable, &conceallevel, \
                    &winbar, maparg(':', 'n')]";
        let want = Value::from(vec![
            Value::from(0),
            Value::from(0),
            Value::from(0),
            Value::from(0),
            Value::from(0),
            Value::from(0),
            Value::from(""),
            Value::from("<Nop>"),
        ]);
        assert!(e.settles(held, want.clone()).await);
        e.exec("vim.wo.number = true; vim.wo.signcolumn = 'yes'", vec![])
            .await
            .unwrap();
        assert!(
            e.settles(
                "[&number, &signcolumn]",
                Value::from(vec![Value::from(0), Value::from("no")])
            )
            .await
        );
    }

    #[tokio::test]
    async fn the_window_s_keys_scroll_it_whatever_the_user_mapped_them_to() {
        let (e, _init) = start_configured(
            r#"
            vim.keymap.set("n", "<C-e>", "<Nop>")
            vim.keymap.set("n", "<C-y>", "<Nop>")
            "#,
        )
        .await;
        let text: Vec<String> = (1..=10).map(|n| n.to_string()).collect();
        e.show(&text, &[], (0, 0), true, "#101010").await.unwrap();
        e.press("3<C-e>").await.unwrap();
        assert!(e.settles("line('w0')", Value::from(4)).await);
        e.press("2<C-y>").await.unwrap();
        assert!(e.settles("line('w0')", Value::from(2)).await);
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
                e.settles(LAYOUT, alone()).await,
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

//! The window's stylesheet, and the height of a terminal row: as tall as a
//! line of the console's font, measured each time the font changes.

use dioxus::prelude::*;

/// A line of the built-in font, until the font is measured.
pub const DEFAULT_CELL_HEIGHT: f64 = 17.0;

/// How tall a line of the console is, handed to the components that draw
/// cells through the context.
#[derive(Clone, Copy)]
pub struct CellHeight(pub Signal<f64>);

/// How tall a line of the console is, drawn again when it is measured anew.
pub fn use_cell_height() -> f64 {
    use_context::<CellHeight>().0()
}

pub const CSS: &str = r#"
:root {
  color-scheme: dark;
  --ground: #0f1114;
  --panel: #16191d;
  --raised: #1d2126;
  --select: #252a31;
  --line: #22262c;
  --line-strong: #2f353d;
  --text: #e7e8eb;
  --text-2: #a2a8b1;
  --text-3: #868c96;
  --wait: #e8b45e;
  --run: #78aee8;
  --done: #8fc6a4;
  --fail: #eba3aa;
  --coord: #b9a2ec;
  --mono: "SF Mono", Menlo, ui-monospace, "Hiragino Kaku Gothic ProN", monospace;
  /* The console's font and line height, set from the settings. */
  --term-font: var(--mono); --term-size: 13px; --term-weight: 400; --cell-h: 17px;
  --sans: -apple-system, BlinkMacSystemFont, "Hiragino Sans", "Noto Sans JP", sans-serif;
}
* { box-sizing: border-box; }
html, body { margin: 0; height: 100%; background: var(--ground); color: var(--text);
  font-family: var(--sans); font-size: 13px; overflow: hidden; user-select: none; }
#main { height: 100%; }
#adroit-keys { position: fixed; left: 0; top: 0; width: 1px; height: var(--cell-h); opacity: 0; z-index: 50; pointer-events: none;
  margin: 0; padding: 0; border: 0; outline: none; resize: none; overflow: hidden; white-space: pre;
  font-family: var(--term-font); font-size: var(--term-size); font-weight: var(--term-weight); line-height: var(--cell-h); color: var(--text); background: var(--raised); caret-color: transparent; }
#adroit-keys.composing { opacity: 1; caret-color: var(--text); }
#adroit-keys.composing.gridded { opacity: 0; }
#adroit-compose { position: fixed; display: none; height: var(--cell-h); z-index: 50; pointer-events: none; white-space: pre;
  font-family: var(--term-font); font-size: var(--term-size); font-weight: var(--term-weight); line-height: var(--cell-h); color: var(--text); background: var(--raised); }
#adroit-compose.on { display: flex; }
/* The underline spans each character's cells, not its glyph, which a wide
   character leaves short of its two cells. */
#adroit-compose span { display: inline-block; flex: none; overflow: hidden; height: calc(var(--cell-h) - 1px); box-shadow: inset 0 -1px 0 currentColor; }
.app { height: 100%; display: flex; flex-direction: column; }
.header { height: 40px; display: flex; align-items: center; gap: 14px; padding: 0 16px; flex: none; }
.header-crumbs { display: flex; gap: 6px; color: var(--text-2); align-items: center; min-width: 0; overflow: hidden; white-space: nowrap; }
.header-crumbs-sep { color: #5d636c; }
.header-crumbs-path { font-family: var(--mono); font-size: 12px; }
.flex-spacer { flex: 1; }
.header-chip { display: inline-flex; align-items: center; gap: 6px; height: 22px; padding: 0 10px; border-radius: 11px;
  background: var(--panel); color: var(--text-2); font-size: 12px; }
.header-chip.wait { background: rgba(232,180,94,0.12); color: var(--wait); font-weight: 500; }
.app-body { flex: 1; min-height: 0; display: flex; padding: 0 8px; }
.app-body.dragging, .app-body.dragging * { cursor: col-resize !important; user-select: none; }
.panel-splitter { flex: none; width: 8px; cursor: col-resize; position: relative; }
.panel-splitter::after { content: ""; position: absolute; top: 20%; bottom: 20%; left: 3px; width: 2px; border-radius: 1px; background: transparent; transition: background 0.15s; }
.panel-splitter:hover::after, .app-body.dragging .panel-splitter::after { background: var(--line-strong); }
.pane-toggle { display: inline-flex; align-items: center; justify-content: center; width: 26px; height: 24px; border-radius: 6px; color: var(--text-3); cursor: default; }
.pane-toggle:hover { background: var(--select); color: var(--text-2); }
.pane-toggle.on { color: var(--text-2); }
[hidden] { display: none !important; }
.app-panel { background: var(--panel); border: 1px solid var(--line); border-radius: 10px; display: flex; flex-direction: column; overflow: hidden; }
.app-panel.focused { border-color: #3a4352; }
.sidebar { flex: none; }
.center-panel { flex: 1 1 0; min-width: 0; }
.details { flex: none; }
.details-scroll { flex: 1; min-height: 0; overflow-y: auto; }
.details-scroll .list-rows { overflow: visible; }
.sidebar-title { font-size: 12px; font-weight: 500; color: var(--text-2); padding: 12px 16px 6px; display: flex; justify-content: space-between; gap: 8px; white-space: nowrap; }
.filter-field { margin: 0 12px 6px; padding: 4px 8px; border-radius: 6px; background: var(--raised); font-family: var(--mono); font-size: 12.5px; color: var(--text); }
.filter-field { display: flex; align-items: center; gap: 6px; cursor: text; border: 1px solid transparent; }
.filter-field.on { border-color: var(--line-strong); }
.filter-field-funnel { display: inline-flex; color: var(--text-3); }
.filter-field-input::placeholder { color: var(--text-3); font-family: var(--sans); font-size: 12px; }
.filter-field-input { flex: 1; -webkit-user-select: text; user-select: text; min-width: 0; padding: 0; border: 0; outline: none; background: transparent; color: inherit; font: inherit; }
.list-rows { overflow-y: auto; padding: 0 8px 8px; }
.list-row { display: flex; align-items: center; gap: 8px; height: 26px; padding: 0 8px; border-radius: 6px; cursor: default; white-space: nowrap; overflow: hidden; }
.list-row-name { flex: 1; overflow: hidden; text-overflow: ellipsis; }
.list-row-meta { color: var(--text-3); font-size: 11.5px; }
.list-row.selected { background: var(--select); }
.list-row.shown { box-shadow: inset 3px 0 0 var(--run); background: rgba(120,174,232,0.08); }
.list-row.shown.selected { background: var(--select); }
.list-row.shown .list-row-name { color: var(--text); font-weight: 600; }
.app-panel.focused .list-row.selected.shown { box-shadow: inset 3px 0 0 var(--run), inset 0 0 0 1px #3a4352; }
.app-panel.focused .list-row.selected { box-shadow: inset 0 0 0 1px #3a4352; }
.list-row.head { font-weight: 500; }
.list-row.vacant { opacity: 0.5; }
.list-row.dim { color: var(--text-2); }
.list-row.mono { font-family: var(--mono); font-size: 12px; }
.sidebar-inbox .list-row { height: auto; padding: 7px 10px; align-items: flex-start; }
.sidebar-inbox-text { display: flex; flex-direction: column; gap: 2px; flex: 1; min-width: 0; }
.sidebar-inbox-sub { color: var(--text-2); font-size: 12px; overflow: hidden; text-overflow: ellipsis; }
.sidebar-divider { height: 1px; background: var(--line); margin: 4px 16px; flex: none; }
.state-glyph { flex: none; width: 8px; height: 8px; display: inline-block; }
.state-glyph.wait { background: var(--wait); border-radius: 1.5px; transform: rotate(45deg); width: 7px; height: 7px; }
.state-glyph.run { background: var(--run); border-radius: 50%; }
.state-glyph.done { color: var(--done); width: auto; height: auto; font-size: 11px; }
.state-glyph.idle { border: 1.5px solid var(--text-3); border-radius: 50%; }
.state-glyph.fail { color: var(--fail); width: auto; height: auto; font-size: 11px; }
.state-glyph.stop { background: var(--text-3); width: 7px; height: 7px; }
.reads-only-mark { flex: none; color: var(--text-3); display: inline-flex; }
.sidebar-inbox .list-row-name .reads-only-mark { margin-left: 6px; vertical-align: -1px; }
/* One line however many tabs there are, so the console keeps its height; the rest scrolls sideways. */
.tabs { display: flex; flex-wrap: nowrap; overflow-x: auto; scrollbar-width: none; align-items: center; gap: 4px; padding: 8px 10px; border-bottom: 1px solid var(--line); flex: none; }
.tabs::-webkit-scrollbar { display: none; }
.tabs > * { flex: none; }
.tab { white-space: nowrap; display: inline-flex; align-items: center; gap: 8px; height: 28px; padding: 0 12px; border-radius: 7px; color: var(--text-2); cursor: default; }
.tab.active { background: var(--select); color: var(--text); font-weight: 500; }
.tab.shell { font-family: var(--mono); font-size: 12px; }
.tabs-sep { width: 1px; height: 16px; background: #2a2f36; margin: 0 6px; }
.tabs-where { color: var(--text-3); font-size: 11.5px; margin-right: 6px; font-family: var(--mono); }
.console { flex: 1; min-height: 0; position: relative; }
.console-screen { position: absolute; inset: 0; padding: 10px 12px 0; overflow: hidden; }
.console-clicks { position: absolute; top: 10px; left: 12px; right: 0; bottom: 0; }
.console-reader-line { position: absolute; left: 12px; right: 12px; z-index: 1; background: var(--console-bg); font-family: var(--term-font); font-size: var(--term-size); font-weight: var(--term-weight); line-height: var(--cell-h); height: var(--cell-h); color: var(--text); white-space: pre; overflow: hidden; }
.console-empty { position: absolute; inset: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px;
  color: var(--text-2); text-align: center; padding: 40px; line-height: 1.7; }
.console-empty-title { color: var(--text); font-size: 15px; font-weight: 500; }
.console > .console-empty { bottom: var(--composer, 0px); overflow-y: auto; justify-content: safe center; padding: 24px 32px; }
.console-empty.detached { background: rgba(15,17,20,0.92); }
.console-empty code, .console-empty-hint code { font-family: var(--mono); font-size: 12px; background: var(--raised); padding: 1px 6px; border-radius: 4px; }
.screen { position: relative; font-family: var(--term-font); font-size: var(--term-size); font-weight: var(--term-weight); line-height: var(--cell-h); white-space: pre; }
.screen-line { height: var(--cell-h); display: flex; }
.screen span { display: inline-block; overflow: hidden; flex: none; }
.screen-picture { position: absolute; overflow: hidden; pointer-events: none; }
.screen-picture img { position: absolute; max-width: none; image-rendering: auto; }
.screen-cursor { position: absolute; background: rgba(231,232,235,0.85); mix-blend-mode: difference; pointer-events: none; }
.cell-probe { position: absolute; visibility: hidden; font-family: var(--term-font); font-size: var(--term-size); font-weight: var(--term-weight); line-height: var(--cell-h); white-space: pre; }
.prompt { position: absolute; left: 14px; right: 14px; bottom: 14px; background: var(--raised); border: 1px solid var(--line-strong);
  border-radius: 10px; box-shadow: 0 -6px 30px rgba(0,0,0,0.45); }
.prompt.focused { border-color: #4a5566; }
.prompt-foot { display: flex; align-items: center; gap: 10px; padding: 2px 12px 6px; font-size: 11.5px; color: var(--text-3); }
.prompt-foot-target { color: var(--text-2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
.prompt-foot-message { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
.mode-chip { display: inline-flex; align-items: center; height: 20px; padding: 0 8px; border-radius: 5px; font-family: var(--mono); font-weight: 500; font-size: 11px; }
.mode-chip.insert { background: rgba(143,198,164,0.14); color: var(--done); }
.mode-chip.normal { background: rgba(120,174,232,0.14); color: var(--run); }
.mode-chip.pass { background: rgba(232,180,94,0.14); color: var(--wait); }
.mode-chip.cmd { background: rgba(185,162,236,0.14); color: var(--coord); }
.prompt-grid { position: relative; padding: 10px 14px 4px; }
.prompt-scroll { position: absolute; top: 10px; bottom: 4px; right: 4px; width: 4px; border-radius: 2px; background: rgba(255,255,255,0.05); }
.prompt-scroll-thumb { position: absolute; left: 0; right: 0; border-radius: 2px; background: #4a5566; }
.vim-cmdline { font-family: var(--term-font); font-size: var(--term-size); font-weight: var(--term-weight); color: var(--text); padding: 2px 14px; }
.vim-menu { position: absolute; z-index: 10; background: #21262d; border: 1px solid #3b4450;
  border-radius: 8px; box-shadow: 0 10px 30px rgba(0,0,0,0.5); overflow: hidden; }
.vim-menu-item { display: flex; gap: 10px; padding: 7px 12px; align-items: baseline; }
.vim-menu-item.selected { background: #3a4350; box-shadow: inset 2px 0 0 var(--run); }
.vim-menu-word { font-family: var(--mono); font-size: 12.5px; color: var(--text); }
.vim-menu-detail { color: var(--text-3); font-size: 11.5px; margin-left: auto; }
.vim-menu-info { color: var(--text-2); font-size: 11.5px; padding: 0 12px 8px; white-space: pre-wrap; }
.details-sheets { display: flex; gap: 2px; padding: 8px 10px; border-bottom: 1px solid var(--line); flex: none; margin-bottom: 8px; }
.details-sheet { height: 24px; display: inline-flex; align-items: center; padding: 0 10px; border-radius: 6px; color: var(--text-2); font-size: 12px; cursor: default; }
.details-sheet.active { background: var(--select); color: var(--text); font-weight: 500; }
.list-rows.details-git { flex: 1; user-select: text; }
.details-file-status { color: var(--wait); white-space: pre; flex: none; }
.commit { display: grid; grid-template-columns: auto 1fr; gap: 0 8px; padding: 5px 8px; font-size: 12.5px; }
.commit-hash { font-family: var(--mono); font-size: 11.5px; color: var(--run); }
.commit-subject { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.commit-when { grid-column: 2; color: var(--text-3); font-size: 11.5px; }
.diff { font-family: var(--mono); font-size: 11.5px; line-height: 16px; white-space: pre; overflow-x: auto; padding: 0 8px; }
.diff-file { color: var(--text); font-weight: 600; margin-top: 10px; }
.diff-hunk { color: var(--coord); }
.diff-add { color: var(--done); background: rgba(143,198,164,0.07); }
.diff-del { color: var(--fail); background: rgba(235,163,170,0.07); }
.diff-ctx { color: var(--text-2); }
.git-error { color: var(--fail); font-size: 12px; padding: 8px; white-space: pre-wrap; }
.details-block { padding: 0 16px 14px; display: flex; flex-direction: column; gap: 6px; line-height: 1.6; }
.details-label { font-size: 12px; font-weight: 500; color: var(--text-2); }
.details-facts { display: grid; grid-template-columns: max-content 1fr; gap: 2px 12px; font-size: 12.5px; }
.details-facts-key { color: var(--text-3); }
.details-path { font-family: var(--mono); font-size: 11.5px; color: var(--text-3); word-break: break-all; user-select: text; }
.footer { height: 30px; display: flex; align-items: center; gap: 14px; padding: 0 16px; font-size: 11.5px; color: var(--text-3); flex: none; }
.footer { overflow: hidden; }
.footer-status { color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; flex: 0 0 auto; max-width: 70%; }
.footer .kbd-list { white-space: nowrap; flex: 0 1 auto; min-width: 0; overflow: hidden; }
kbd { font-family: var(--mono); font-size: 10.5px; color: #c9ccd2; background: var(--raised); border: 1px solid var(--line-strong);
  border-bottom-width: 2px; border-radius: 4px; padding: 0 5px; line-height: 16px; display: inline-block; }
.kbd-list { display: flex; align-items: center; gap: 5px; }
.direct-banner { position: absolute; top: 10px; left: 14px; right: 14px; z-index: 5; display: flex; gap: 8px; align-items: center;
  padding: 8px 12px; border-radius: 8px; border: 2px solid var(--wait); background: rgba(29,33,38,0.95); color: var(--text-2); font-size: 12px; }
.direct-banner-title { color: var(--wait); font-weight: 600; }
.dialog-scrim { position: fixed; inset: 0; z-index: 20; background: rgba(8,9,11,0.6); display: flex; align-items: center; justify-content: center; }
.dialog { width: 420px; max-width: calc(100% - 32px); background: var(--raised); border: 1px solid var(--line-strong); border-radius: 10px;
  box-shadow: 0 12px 40px rgba(0,0,0,0.55); padding: 18px 20px; display: flex; flex-direction: column; gap: 10px; color: var(--text-2); line-height: 1.6; }
.dialog-title { color: var(--text); font-size: 15px; font-weight: 500; }
.dialog-actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 6px; }
.button { display: inline-flex; border: 0; font: inherit; align-items: center; gap: 8px; height: 28px; padding: 0 12px; border-radius: 7px; background: var(--select); color: var(--text); cursor: default; }
.button.primary { background: rgba(232,180,94,0.16); color: var(--wait); font-weight: 500; }
.button.small { height: 24px; padding: 0 9px; font-size: 12px; }
.button.danger { color: var(--fail); background: rgba(235,163,170,0.10); }
.button.busy { opacity: 0.6; }
.dialog-actions .flex-spacer { flex: 1; }
.sidebar-tool { color: var(--text-2); font-size: 15px; line-height: 1; padding: 0 4px; border-radius: 4px; cursor: default; }
.sidebar-tool:hover { background: var(--select); color: var(--text); }
.sidebar-tools { display: inline-flex; gap: 2px; }
.sidebar-tool { display: inline-flex; align-items: center; justify-content: center; width: 20px; height: 20px; padding: 0; }
.list-row-start { display: inline-flex; align-items: center; justify-content: center; width: 18px; height: 18px; margin-left: auto;
  flex: none; border-radius: 4px; color: var(--text-3); }
.list-row-start:hover { background: var(--line-strong); color: var(--text); }
.sidebar-tool.on { background: var(--select); color: var(--text); }
.tab-close { color: var(--text-3); padding: 0 3px; border-radius: 4px; margin-right: -6px; }
.tab-close:hover { background: var(--line-strong); color: var(--text); }
.context-menu-shade { position: fixed; inset: 0; z-index: 30; }
.context-menu { position: fixed; min-width: 200px; padding: 4px; border-radius: 8px; background: var(--raised);
  border: 1px solid var(--line-strong); box-shadow: 0 8px 24px rgba(0,0,0,0.4); }
.context-menu-item { display: flex; align-items: center; justify-content: space-between; gap: 16px; height: 26px;
  padding: 0 10px; border-radius: 5px; color: var(--text); font-size: 12.5px; cursor: default; }
.context-menu-item:hover { background: var(--select); }
.console-empty-start { display: flex; flex-direction: column; align-items: center; gap: 10px; margin-top: 8px; max-width: 360px; }
.dialog.rename-dialog .filter-field { margin: 0; min-height: 26px; }
.button:disabled { opacity: 0.5; }
.dialog.picker-dialog { width: 640px; height: min(520px, calc(100% - 48px)); padding: 16px 12px 12px; gap: 8px; }
.dialog.picker-dialog .dialog-title { padding: 0 8px; }
.dialog.picker-dialog .filter-field { margin: 0 8px; }
.dialog.picker-dialog .list-rows { flex: 1; min-height: 0; padding: 0; line-height: normal; }
.dialog.picker-dialog .list-row { color: var(--text-2); }
.dialog.picker-dialog .list-row.selected { color: var(--text); box-shadow: inset 0 0 0 1px #3a4352; }
.dialog.picker-dialog .list-row-name { min-width: 0; }
.picker-chevron { flex: none; width: 10px; color: var(--text-3); font-size: 10px; }
.picker-match { color: var(--wait); font-weight: 600; }
.picker-place { margin-left: 10px; color: var(--text-3); font-size: 11.5px; }
.picker-keys { display: flex; flex-wrap: wrap; gap: 4px 14px; padding: 6px 8px 0; border-top: 1px solid var(--line); font-size: 11.5px; color: var(--text-3); }
"#;

/// The Settings window's stylesheet, on the main window's colours.
pub const SETTINGS_CSS: &str = r#"
:root {
  color-scheme: dark;
  --ground: #0f1114; --panel: #16191d; --raised: #1d2126; --select: #252a31; --line: #22262c; --line-strong: #2f353d;
  --text: #e7e8eb; --text-2: #a2a8b1; --text-3: #868c96; --wait: #e8b45e; --run: #78aee8; --fail: #eba3aa;
  --mono: "SF Mono", Menlo, ui-monospace, "Hiragino Kaku Gothic ProN", monospace;
  --sans: -apple-system, BlinkMacSystemFont, "Hiragino Sans", "Noto Sans JP", sans-serif;
}
* { box-sizing: border-box; }
html, body { margin: 0; height: 100%; background: var(--ground); color: var(--text);
  font-family: var(--sans); font-size: 13px; overflow: hidden; user-select: none; cursor: default; }
#main { height: 100%; }
.settings { height: 100%; display: flex; }
.settings-nav { flex: none; width: 188px; padding: 16px 10px; background: var(--panel); border-right: 1px solid var(--line); display: flex; flex-direction: column; gap: 2px; }
.settings-nav-item { padding: 6px 10px; border-radius: 6px; color: var(--text-2); }
.settings-nav-item:hover { background: var(--raised); color: var(--text); }
.settings-nav-item.on { background: var(--select); color: var(--text); font-weight: 500; }
.settings-pane { flex: 1; min-width: 0; overflow-y: auto; padding: 20px 28px 28px; }
h1 { font-size: 18px; font-weight: 600; margin: 0 0 16px; }
h2 { font-size: 12px; font-weight: 500; color: var(--text-2); margin: 0 0 4px; padding: 0 14px; }
.settings-group { background: var(--panel); border: 1px solid var(--line); border-radius: 10px; padding: 6px 0; margin-bottom: 18px; }
.settings-group h2 { padding-top: 8px; }
.settings-row { display: flex; align-items: center; gap: 16px; padding: 10px 14px; }
.settings-row + .settings-row { border-top: 1px solid var(--line); }
.settings-row-label { flex: 1; min-width: 0; }
.settings-hint { color: var(--text-3); font-size: 12px; margin-top: 2px; overflow: hidden; text-overflow: ellipsis; }
.settings-mono { font-family: var(--mono); font-size: 12px; }
.settings-row-control { flex: none; display: flex; gap: 8px; align-items: center; }
select, input, button { font: inherit; font-size: 13px; color: var(--text); background: var(--ground);
  border: 1px solid var(--line-strong); border-radius: 6px; padding: 4px 8px; outline: none; }
input[type=number] { width: 72px; }
select:focus, input:focus { border-color: var(--run); }
button { background: var(--select); border-color: transparent; padding: 5px 12px; }
button:hover { background: #2d333b; }
button.primary { background: rgba(232,180,94,0.16); color: var(--wait); font-weight: 500; }
.settings-actions { display: flex; justify-content: flex-end; }
.settings-error { color: var(--fail); margin-bottom: 12px; user-select: text; }
.settings-empty { color: var(--text-3); padding: 12px 14px; }
.keys-bar { display: flex; gap: 8px; margin-bottom: 12px; }
.keys-bar-search { flex: 1; min-width: 0; }
table.keys-table { width: 100%; border-collapse: collapse; background: var(--panel); border: 1px solid var(--line); border-radius: 10px; overflow: hidden; }
table.keys-table th { text-align: left; font-weight: 500; font-size: 12px; color: var(--text-2); padding: 8px 12px; border-bottom: 1px solid var(--line); }
table.keys-table td { padding: 5px 12px; border-top: 1px solid var(--line); }
table.keys-table .keys-table-area { color: var(--text-3); }
kbd { font-family: var(--mono); font-size: 10.5px; color: #c9ccd2; background: var(--raised); border: 1px solid var(--line-strong);
  border-radius: 4px; padding: 0 5px; margin-right: 3px; }
.keys-area { padding: 4px 0 6px; }
.keys-area-head { padding: 8px 14px 6px; }
.keys-area-head h2 { padding: 0; margin: 0; }
.binding { display: flex; align-items: center; gap: 12px; padding: 6px 14px; border-top: 1px solid var(--line); }
.binding:hover { background: var(--raised); }
.binding-keys { flex: 0 0 200px; }
.binding-action { flex: 1; min-width: 0; color: var(--text); }
.binding-mark { font-size: 11px; color: var(--wait); background: rgba(232,180,94,0.12); border-radius: 4px; padding: 1px 6px; }
.binding.gone .binding-action, .binding.gone .binding-keys { opacity: 0.5; text-decoration: line-through; }
.binding.gone .binding-mark { color: var(--text-3); background: var(--raised); }
button.settings-link { background: none; border: none; color: var(--run); padding: 2px 4px; }
button.settings-link:hover { background: none; text-decoration: underline; }
button.danger { color: var(--fail); background: rgba(235,163,170,0.10); }
button:disabled { opacity: 0.45; }
.binding-add { border-top: 1px solid var(--line); padding: 4px 10px 0; }
.binding-form { border-top: 1px solid var(--line); background: var(--raised); padding: 10px 14px; display: flex; flex-direction: column; gap: 8px; }
.binding-fields { display: flex; gap: 8px; align-items: center; }
.binding-fields .binding-recorder { flex: 1; min-width: 0; }
.binding-recorder.recording { border-color: var(--wait); box-shadow: 0 0 0 2px rgba(232,180,94,0.2); }
.binding-action-select { flex: 0 0 220px; }
.binding-note { font-size: 12px; color: var(--text-3); }
.binding-note.warn { color: var(--wait); }
.binding-note.bad { color: var(--fail); }
.binding-actions { display: flex; gap: 8px; align-items: center; }
.binding-actions-spacer { flex: 1; }
"#;

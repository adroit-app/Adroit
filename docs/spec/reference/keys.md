# Key reference

<!-- Written by `just references` from the code; edit the code, not this file. -->

Every action a binding can name, what it does, what the menu bar calls it,
and the keys Adroit binds it to in each area before a file of bindings
changes them ([keys](../keys.md)).

## Actions

| Action | Menu bar | Does | Default keys |
| --- | --- | --- | --- |
| `none` |  | nothing; bound to keys, it takes them away |  |
| `window.left` | Left Window | move to the window to the left: the tree lies left of the content's windows | `global`: `Ctrl+q h`, `Ctrl+q ArrowLeft` |
| `window.down` | Window Below | move to the window below: a session's prompt lies below its console | `global`: `Ctrl+q j`, `Ctrl+q ArrowDown` |
| `window.up` | Window Above | move to the window above: a session's console lies above its prompt | `global`: `Ctrl+q k`, `Ctrl+q ArrowUp` |
| `window.right` | Right Window | move to the window to the right: the details panel lies right of the content's windows | `global`: `Ctrl+q l`, `Ctrl+q ArrowRight` |
| `window.next` | Next Window | move to the window after the one the keys are in, from the tree through the content's windows, each console before its prompt, to the details panel, going round at the end; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+q q` |
| `window.prev` | Previous Window | move to the window before the one the keys are in, in the order `window.next` goes, going round at the start; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+q Q` |
| `window.top` | First Window | move to the first window, the tree when shown | `global`: `Ctrl+q t` |
| `window.bottom` | Last Window | move to the last window, the details panel when shown | `global`: `Ctrl+q b` |
| `window.back` | Back to Window Before | move back to the window the keys were in before | `global`: `Ctrl+q p` |
| `window.split` | Split One Above the Other | split the focused window one above the other, the new window, below it, showing the same ([content](../content.md)) | `global`: `Ctrl+q s` |
| `window.vsplit` | Split Side by Side | split the focused window side by side, the new window, right of it, showing the same ([content](../content.md)) | `global`: `Ctrl+q v` |
| `window.new` | Split for a New Session | split the focused window one above the other and start writing a new session in the new one, in the worktree the focused window shows | `global`: `Ctrl+q n` |
| `window.quit` | Close Split | close the focused window, and its tab when it was the last; what it showed runs on | `global`: `Ctrl+q c`<br>`viewer`: `q` |
| `window.only` | Close Other Splits | close every other window of the tab | `global`: `Ctrl+q o` |
| `window.move_left` | Move Window to the Left | move the focused window to the left of its tab, as tall as the tab | `global`: `Ctrl+q H` |
| `window.move_down` | Move Window to the Bottom | move the focused window to the bottom of its tab, as wide as the tab | `global`: `Ctrl+q J` |
| `window.move_up` | Move Window to the Top | move the focused window to the top of its tab, as wide as the tab | `global`: `Ctrl+q K` |
| `window.move_right` | Move Window to the Right | move the focused window to the right of its tab, as tall as the tab | `global`: `Ctrl+q L` |
| `window.exchange` | Exchange Windows | exchange the focused window with the next one in its split, or the one before the last | `global`: `Ctrl+q x` |
| `window.rotate` | Rotate Windows | rotate the windows of the focused window's split forward, the last becoming the first | `global`: `Ctrl+q r` |
| `window.equalize` | Equal Sizes | give the windows of every split the same size | `global`: `Ctrl+q =` |
| `window.narrower` | Narrower | make the focused window a step narrower; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+q <` |
| `window.wider` | Wider | make the focused window a step wider; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+q >` |
| `window.taller` | Taller | make the focused window a step taller; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+q Plus` |
| `window.shorter` | Shorter | make the focused window a step shorter; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+q -` |
| `window.maximize_height` | Tallest | make the focused window as tall as the others allow | `global`: `Ctrl+q _` |
| `window.maximize_width` | Widest | make the focused window as wide as the others allow | `global`: `Ctrl+q \|` |
| `window.to_tab` | Move Window to a New Tab | move the focused window to a new tab, unless it is the only one of its tab | `global`: `Ctrl+q T` |
| `tab.next` | Next Tab | show the next tab of the area the keys are in: the details panel's sheets there, elsewhere the content's tabs; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+Tab`, `Ctrl+PageDown`, `Cmd+}`, `Ctrl+q g t` |
| `tab.prev` | Previous Tab | show the previous tab of the area the keys are in: the details panel's sheets there, elsewhere the content's tabs; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+Shift+Tab`, `Ctrl+PageUp`, `Cmd+{`, `Ctrl+q g T` |
| `tab.last` | Tab Before | show the content's tab shown before the one shown now | `global`: `Ctrl+q g Tab` |
| `cursor.down` |  | move the selection in the tree to the next row; in the details panel, in the list it shows, or scroll what else it shows | `tree`: `j`, `ArrowDown`<br>`details`: `j`, `ArrowDown` |
| `cursor.up` |  | move the selection in the tree to the previous row; in the details panel, in the list it shows, or scroll what else it shows | `tree`: `k`, `ArrowUp`<br>`details`: `k`, `ArrowUp` |
| `cursor.first` |  | move the selection in the tree to the first row; in the details panel, in the list it shows, or scroll what else it shows | `tree`: `g g`<br>`details`: `g g` |
| `cursor.last` |  | move the selection in the tree to the last row; in the details panel, in the list it shows, or scroll what else it shows | `tree`: `G`<br>`details`: `G` |
| `cursor.go` |  | open the selected session, shell, worktree or file: go to a window showing it, in any tab, keeping the preview when that shows it, or else open it in a new tab after the one shown, and move to it, or to the prompt of a session not started yet; on a folded row, a worktree or a session holding others too, unfold it instead; fold or unfold a workspace, a heading or a directory; in the details panel, open the selected row, an image of Resources as a file is opened, the keys staying in the panel | `tree`: `Enter`, `o` |
| `cursor.split` |  | open the selected session, shell, worktree or file as `cursor.go` does, in a new window split off the focused one, below it; in the details panel, the selected image of Resources | `tree`: `s` |
| `cursor.vsplit` |  | open the selected session, shell, worktree or file as `cursor.go` does, in a new window split off the focused one, beside it; in the details panel, the selected image of Resources | `tree`: `v` |
| `cursor.tab` |  | open the selected session, shell, worktree or file as `cursor.go` does, in a new tab after the one shown, though a window shows it already; in the details panel, the selected image of Resources | `tree`: `t` |
| `cursor.show` |  | look at the selected session, shell, worktree or file: show the tab already showing it and close the preview, or else show it in the preview, with the keys staying in the tree; go into a workspace, a heading, a directory or a folded row instead: unfold it and go to its first row once it is listed; in the details panel, look at the selected image of Resources in the preview, the keys staying in the panel, or open any other row | `tree`: `l`, `L`, `Shift+Enter`, `Ctrl+Enter` |
| `cursor.fold` |  | fold the selected node when it is open, or else the row it sits under, going there; in the details panel, go back out to the list what it shows was opened from | `tree`: `h`<br>`details`: `Escape`, `Backspace` |
| `cursor.close` |  | close every window showing the selected session, shell or Neovim, or the selected file in its viewer or its Neovim; what they showed runs on | `tree`: `x` |
| `cursor.copy` |  | copy the path of the selected file, worktree or workspace, or the name of the selected session, shell or Neovim; in the details panel, the selected link of the overview: its address, or its file's path | `tree`: `y`<br>`details`: `y` |
| `cursor.reveal` |  | in the details panel, show the selected file of the overview in the Finder |  |
| `filer.down` |  | move to the next file or directory, in the tree's files or the picker's directories | `filer`: `j`, `ArrowDown` |
| `filer.up` |  | move to the previous file or directory | `filer`: `k`, `ArrowUp` |
| `filer.into` |  | open the directory selected and go to its first entry once it is read; on a file of the tree, look at it as `cursor.show` does | `filer`: `l`, `ArrowRight` |
| `filer.out` |  | close the directory selected when it is open, or else the one it is in, going there | `filer`: `h`, `ArrowLeft` |
| `filer.open` |  | in the tree, open the file or fold the directory selected as `cursor.go` does; in the picker, start a session in the directory selected | `filer`: `Enter` |
| `filer.hidden` |  | show or hide the files and directories whose names start with `.` | `filer`: `!` |
| `waiting.next` | Next Waiting | look at the next session waiting on you in the preview, with the keys in its console; repeats ([keys](../keys.md#repeating)) | `global`: `Mod+j` |
| `waiting.prev` | Previous Waiting | look at the previous session waiting on you in the preview, with the keys in its console; repeats ([keys](../keys.md#repeating)) | `global`: `Mod+k` |
| `session.next` | Next Session | look at the session after the one shown, in the tree's order over every session, folded away or filtered out too, going round at the end, in the preview; the keys stay in the area they are in; repeats ([keys](../keys.md#repeating)) |  |
| `session.prev` | Previous Session | look at the session before the one shown, in the tree's order over every session, folded away or filtered out too, going round at the start, in the preview; the keys stay in the area they are in; repeats ([keys](../keys.md#repeating)) |  |
| `workspace.next` | Next Workspace | look at a session of the workspace after the one shown, in the tree's order over every workspace holding a session, folded away or filtered out too, going round at the end: the session last shown there since the window opened, or else its first, in the preview; the keys stay in the area they are in; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+q }`<br>`tree`: `}` |
| `workspace.prev` | Previous Workspace | look at a session of the workspace before the one shown, as `workspace.next` does, going round at the start; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+q {`<br>`tree`: `{` |
| `session.pick` | Go to or Start Session… | open the picker, to go to a session or choose where a new one works ([details](../tree.md#going-to-a-session-or-starting-one)) | `global`: `Mod+p`<br>`tree`: `w` |
| `session.new` | New Session | start a session, with the prompt ready to write its first message: in the selected row's worktree or directory from the tree, in the one the focused window shows from elsewhere | `global`: `Mod+t`<br>`tree`: `n` |
| `session.scratch` | New Scratch Session | start a scratch session, with the prompt ready: one that runs in a new directory of its own, for talking about something no project holds ([details](../tree.md#scratch-sessions)) |  |
| `session.resume` | Resume Conversation… | list the conversations Claude Code and Codex kept, to resume one in a new session ([details](../sessions.md#resuming-a-past-conversation)) | `global`: `Mod+o`<br>`tree`: `O` |
| `session.stop` | End… | end a session Adroit runs after asking; Claude Code keeps its conversation, for `/resume` in a session started later | `global`: `Mod+Backspace`<br>`tree`: `X`<br>`console`: `X` |
| `session.rename` | Rename… | edit the name of a session Adroit runs in place; the tree shows it, and the session is sent Claude Code's `/rename` so its own interface says it too | `tree`: `R` |
| `session.profile_next` | Next Profile of New Session | while a session not started yet is shown, switch the profile it is to start from to the next one ([details](../sessions.md#agents)) | `draft`: `Ctrl+g p` |
| `session.profile_prev` | Previous Profile of New Session | while a session not started yet is shown, switch the profile it is to start from to the previous one ([details](../sessions.md#agents)) | `draft`: `Ctrl+g P` |
| `shell.open` | Open Shell | open a shell in the worktree | `global`: `Mod+s`<br>`tree`: `$` |
| `terminal.stop` | Stop Terminal | end the terminal selected in the tree, or else the one shown |  |
| `neovim.open` | Open Neovim | open Neovim, with your own configuration, in the worktree as `shell.open` opens a shell ([shells](../content.md#terminals)) | `global`: `Mod+e`<br>`tree`: `e` |
| `tab.close` | Close Tab | close the content's tab shown and every window in it; agents and terminals keep running and remain reachable from the tree | `global`: `Mod+w` |
| `window.close` | Close Window | close the window; the instance's other windows stay open, and its sessions run on | `global`: `Cmd+W` |
| `window.open` | New Window | open another window of the instance ([windows](../sessions.md#windows)) | `global`: `Mod+n` |
| `refresh` | Refresh | read the sessions, and what git says of where they run, again now | `global`: `Mod+r`<br>`tree`: `r` |
| `filter` | Filter… | narrow the tree to the sessions, shells, workspaces and worktrees whose names hold what is typed next; `Enter` keeps it, `Escape` clears it | `global`: `Mod+f`<br>`tree`: `/` |
| `details.overview` | Overview | show the overview sheet of the details panel ([details](../details.md)), and the panel when it is hidden | `details`: `1` |
| `details.changes` | Changes | show the changes sheet of the details panel ([details](../details.md)), and the panel when it is hidden | `details`: `2` |
| `details.log` | Log | show the log sheet of the details panel ([details](../details.md)), and the panel when it is hidden | `details`: `3` |
| `details.agents` | Agents | show the agents sheet of the details panel ([details](../details.md)), and the panel when it is hidden | `details`: `4` |
| `details.resources` | Resources | show the resources sheet of the details panel ([details](../details.md)), and the panel when it is hidden | `details`: `5` |
| `worktrees.toggle` | Toggle All Worktrees | list every worktree git knows, those without sessions too, or only those with sessions again | `tree`: `!` |
| `scroll.down` |  | scroll the details panel down a step; bound in the console, scroll it: Claude Code and other programs that read the mouse are sent the wheel, others are scrolled back through what scrolled off; bound in a viewer, scroll what it shows | `viewer`: `j`, `ArrowDown` |
| `scroll.up` |  | scroll up a step, what `scroll.down` scrolls | `viewer`: `k`, `ArrowUp` |
| `scroll.half_down` |  | scroll down half a page, what `scroll.down` scrolls | `details`: `Ctrl+d`<br>`viewer`: `Ctrl+d` |
| `scroll.half_up` |  | scroll up half a page, what `scroll.down` scrolls | `details`: `Ctrl+u`<br>`viewer`: `Ctrl+u` |
| `scroll.top` |  | scroll to the top, what `scroll.down` scrolls | `viewer`: `g g` |
| `scroll.bottom` |  | scroll to the bottom, what `scroll.down` scrolls | `viewer`: `G` |
| `console.send_next` |  | send the next key as it is, whatever it is bound to | `console`: `Ctrl+g Ctrl+o` |
| `console.pass_every_key` |  | send every key as it is until `Escape`; `Ctrl+c` sends `Escape` meanwhile | `console`: `Ctrl+g i`, `Ctrl+g Ctrl+i` |
| `prompt.focus` | Prompt | give the prompt the keys |  |
| `prompt.write` |  | give the prompt the keys, and the key pressed with them | `tree`: `i`, `a`<br>`console`: `i`, `a`, `I`, `A`, `o`, `O` |
| `prompt.send` | Send Prompt | send what the prompt holds |  |
| `prompt.images` | Select Attached Image | select an attached image | `prompt`: `Mod+i` |
| `prompt.take_suggestion` |  | on an empty prompt, send the key pressed to the session and then bring the session's input into the prompt, so a prompt Claude Code suggests is taken to edit; an agent whose input cannot be taken back gets the key alone. On a prompt holding text or images, the key, Tab when none, is typed into Neovim |  |
| `prompt.history_prev` |  | put the prompt sent before the one shown in the prompt, to change and send again: those Claude Code recalls with its Up arrow where the focused window's session works | `console`: `ArrowUp`, `Ctrl+p` |
| `prompt.history_next` |  | put the prompt sent after the one shown in the prompt, as `prompt.history_prev` does | `console`: `ArrowDown`, `Ctrl+n` |
| `settings.open` | Settings… | open the Settings window | `global`: `Mod+,` |
| `daemon.restart` | Restart Session Daemon… | restart the sessions' daemon after asking, ending every session it holds ([details](../sessions.md#the-hosts)) |  |
| `keys.help` | Keyboard Shortcuts | show every binding in force, area by area, the areas the keys are in first, each with what its action does and whether it repeats; `Escape`, `q` or the key that showed it closes it, and no other key acts behind it | `global`: `F1`<br>`prompt`: `F1` |
| `zoom.in` | Zoom In | draw the window a step larger | `global`: `Mod+=`, `Mod+Plus` |
| `zoom.out` | Zoom Out | draw the window a step smaller | `global`: `Mod+-` |
| `zoom.reset` | Actual Size | draw the window at the zoom the settings give | `global`: `Mod+0` |
| `tree.toggle` | Toggle Tree | show or hide the tree | `global`: `Mod+b` |
| `details.toggle` | Toggle Details | show or hide the details panel | `global`: `Mod+d` |
| `viewer.zoom_in` |  | in a viewer, draw an image, a PDF or an HTML page larger ([viewers](../content.md#viewers)) | `viewer`: `Plus`, `=` |
| `viewer.zoom_out` |  | in a viewer, draw an image, a PDF or an HTML page smaller | `viewer`: `-` |
| `viewer.zoom_fit` |  | in a viewer, draw an image, a PDF or an HTML page fitting the window | `viewer`: `0` |
| `viewer.page_next` |  | in a viewer, show a PDF's next page | `viewer`: `J` |
| `viewer.page_prev` |  | in a viewer, show a PDF's previous page | `viewer`: `K` |
| `viewer.play` |  | in a viewer, play or pause a movie | `viewer`: `Space` |
| `viewer.back` |  | in a viewer, seek a movie back | `viewer`: `h` |
| `viewer.forward` |  | in a viewer, seek a movie forward | `viewer`: `l` |
| `viewer.copy_path` |  | in a viewer, copy the path of the file it shows | `viewer`: `y` |
| `console.send [<key>]` |  | send the key pressed to the session; with a key named after it, as in `console.send Escape`, send that key instead | `global`: `Ctrl+q .` (`console.send Ctrl+q`)<br>`console`: `Enter`, `ArrowLeft`, `ArrowRight`, `Tab`, `Shift+Tab`, `F2`, `Ctrl+l`, `Ctrl+o`, `Ctrl+t`, `Ctrl+_`, `Ctrl+-` (`console.send Ctrl+_`), `Ctrl+c` (`console.send Escape`), `g Ctrl+c` (`console.send Ctrl+c`) |
| `tab.go <n>` |  | show the content's tab numbered `n`, from 1 to 8, or the last when there are fewer; 9 shows the last however many there are | `global`: `Ctrl+q 1` (`tab.go 1`), `Ctrl+q 2` (`tab.go 2`), `Ctrl+q 3` (`tab.go 3`), `Ctrl+q 4` (`tab.go 4`), `Ctrl+q 5` (`tab.go 5`), `Ctrl+q 6` (`tab.go 6`), `Ctrl+q 7` (`tab.go 7`), `Ctrl+q 8` (`tab.go 8`), `Ctrl+q 9` (`tab.go 9`), `Mod+1` (`tab.go 1`), `Mod+2` (`tab.go 2`), `Mod+3` (`tab.go 3`), `Mod+4` (`tab.go 4`), `Mod+5` (`tab.go 5`), `Mod+6` (`tab.go 6`), `Mod+7` (`tab.go 7`), `Mod+8` (`tab.go 8`), `Mod+9` (`tab.go 9`) |

## Default bindings

```toml
[global]
"Ctrl+q h" = "window.left"
"Ctrl+q ArrowLeft" = "window.left"
"Ctrl+q j" = "window.down"
"Ctrl+q ArrowDown" = "window.down"
"Ctrl+q k" = "window.up"
"Ctrl+q ArrowUp" = "window.up"
"Ctrl+q l" = "window.right"
"Ctrl+q ArrowRight" = "window.right"
"Ctrl+q q" = "window.next"
"Ctrl+q Q" = "window.prev"
"Ctrl+q t" = "window.top"
"Ctrl+q b" = "window.bottom"
"Ctrl+q p" = "window.back"
"Ctrl+q v" = "window.vsplit"
"Ctrl+q s" = "window.split"
"Ctrl+q n" = "window.new"
"Ctrl+q c" = "window.quit"
"Ctrl+q o" = "window.only"
"Ctrl+q H" = "window.move_left"
"Ctrl+q J" = "window.move_down"
"Ctrl+q K" = "window.move_up"
"Ctrl+q L" = "window.move_right"
"Ctrl+q x" = "window.exchange"
"Ctrl+q r" = "window.rotate"
"Ctrl+q =" = "window.equalize"
"Ctrl+q <" = "window.narrower"
"Ctrl+q >" = "window.wider"
"Ctrl+q Plus" = "window.taller"
"Ctrl+q -" = "window.shorter"
"Ctrl+q _" = "window.maximize_height"
"Ctrl+q |" = "window.maximize_width"
"Ctrl+q T" = "window.to_tab"
"Ctrl+Tab" = "tab.next"
"Ctrl+Shift+Tab" = "tab.prev"
"Ctrl+PageDown" = "tab.next"
"Ctrl+PageUp" = "tab.prev"
"Cmd+}" = "tab.next"
"Cmd+{" = "tab.prev"
"Ctrl+q g t" = "tab.next"
"Ctrl+q g T" = "tab.prev"
"Ctrl+q 1" = "tab.go 1"
"Ctrl+q 2" = "tab.go 2"
"Ctrl+q 3" = "tab.go 3"
"Ctrl+q 4" = "tab.go 4"
"Ctrl+q 5" = "tab.go 5"
"Ctrl+q 6" = "tab.go 6"
"Ctrl+q 7" = "tab.go 7"
"Ctrl+q 8" = "tab.go 8"
"Ctrl+q 9" = "tab.go 9"
"Ctrl+q g Tab" = "tab.last"
"Mod+1" = "tab.go 1"
"Mod+2" = "tab.go 2"
"Mod+3" = "tab.go 3"
"Mod+4" = "tab.go 4"
"Mod+5" = "tab.go 5"
"Mod+6" = "tab.go 6"
"Mod+7" = "tab.go 7"
"Mod+8" = "tab.go 8"
"Mod+9" = "tab.go 9"
"Mod+w" = "tab.close"
"Cmd+W" = "window.close"
"Mod+n" = "window.open"
"Ctrl+q }" = "workspace.next"
"Ctrl+q {" = "workspace.prev"
"Mod+j" = "waiting.next"
"Mod+k" = "waiting.prev"
"Mod+p" = "session.pick"
"Ctrl+q ." = "console.send Ctrl+q"
"Mod+t" = "session.new"
"Mod+s" = "shell.open"
"Mod+e" = "neovim.open"
"Mod+o" = "session.resume"
"Mod+Backspace" = "session.stop"
"Mod+f" = "filter"
"Mod+r" = "refresh"
"Mod+," = "settings.open"
"Mod+=" = "zoom.in"
"Mod+Plus" = "zoom.in"
"Mod+-" = "zoom.out"
"Mod+0" = "zoom.reset"
"Mod+b" = "tree.toggle"
"Mod+d" = "details.toggle"
"F1" = "keys.help"

[draft]
"Ctrl+g p" = "session.profile_next"
"Ctrl+g P" = "session.profile_prev"

[tree]
"j" = "cursor.down"
"ArrowDown" = "cursor.down"
"k" = "cursor.up"
"ArrowUp" = "cursor.up"
"g g" = "cursor.first"
"G" = "cursor.last"
"Enter" = "cursor.go"
"o" = "cursor.go"
"l" = "cursor.show"
"s" = "cursor.split"
"v" = "cursor.vsplit"
"t" = "cursor.tab"
"L" = "cursor.show"
"Shift+Enter" = "cursor.show"
"Ctrl+Enter" = "cursor.show"
"h" = "cursor.fold"
"n" = "session.new"
"$" = "shell.open"
"e" = "neovim.open"
"O" = "session.resume"
"x" = "cursor.close"
"X" = "session.stop"
"R" = "session.rename"
"y" = "cursor.copy"
"i" = "prompt.write"
"a" = "prompt.write"
"r" = "refresh"
"/" = "filter"
"w" = "session.pick"
"!" = "worktrees.toggle"
"}" = "workspace.next"
"{" = "workspace.prev"

[filer]
"j" = "filer.down"
"ArrowDown" = "filer.down"
"k" = "filer.up"
"ArrowUp" = "filer.up"
"l" = "filer.into"
"ArrowRight" = "filer.into"
"h" = "filer.out"
"ArrowLeft" = "filer.out"
"Enter" = "filer.open"
"!" = "filer.hidden"

[details]
"j" = "cursor.down"
"ArrowDown" = "cursor.down"
"k" = "cursor.up"
"ArrowUp" = "cursor.up"
"g g" = "cursor.first"
"G" = "cursor.last"
"Escape" = "cursor.fold"
"y" = "cursor.copy"
"Backspace" = "cursor.fold"
"1" = "details.overview"
"2" = "details.changes"
"3" = "details.log"
"4" = "details.agents"
"5" = "details.resources"
"Ctrl+d" = "scroll.half_down"
"Ctrl+u" = "scroll.half_up"

[console]
"Enter" = "console.send"
"ArrowUp" = "prompt.history_prev"
"ArrowDown" = "prompt.history_next"
"ArrowLeft" = "console.send"
"ArrowRight" = "console.send"
"Tab" = "console.send"
"Shift+Tab" = "console.send"
"F2" = "console.send"
"Ctrl+l" = "console.send"
"Ctrl+p" = "prompt.history_prev"
"Ctrl+n" = "prompt.history_next"
"Ctrl+o" = "console.send"
"Ctrl+t" = "console.send"
"Ctrl+_" = "console.send"
"Ctrl+-" = "console.send Ctrl+_"
"Ctrl+c" = "console.send Escape"
"g Ctrl+c" = "console.send Ctrl+c"
"Ctrl+g Ctrl+o" = "console.send_next"
"Ctrl+g i" = "console.pass_every_key"
"Ctrl+g Ctrl+i" = "console.pass_every_key"
"i" = "prompt.write"
"a" = "prompt.write"
"I" = "prompt.write"
"A" = "prompt.write"
"o" = "prompt.write"
"O" = "prompt.write"
"X" = "session.stop"

[prompt]
"Mod+i" = "prompt.images"
"F1" = "keys.help"

[viewer]
"j" = "scroll.down"
"ArrowDown" = "scroll.down"
"k" = "scroll.up"
"ArrowUp" = "scroll.up"
"Ctrl+d" = "scroll.half_down"
"Ctrl+u" = "scroll.half_up"
"g g" = "scroll.top"
"G" = "scroll.bottom"
"Plus" = "viewer.zoom_in"
"=" = "viewer.zoom_in"
"-" = "viewer.zoom_out"
"0" = "viewer.zoom_fit"
"J" = "viewer.page_next"
"K" = "viewer.page_prev"
"Space" = "viewer.play"
"h" = "viewer.back"
"l" = "viewer.forward"
"y" = "viewer.copy_path"
"q" = "window.quit"
```

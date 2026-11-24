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
| `window.edit` |  | switch the focused window between a document's viewer and a Neovim on the document, which runs on hidden while the window shows the viewer ([content](../content.md#viewers)) | `global`: `Ctrl+q e`<br>`viewer`: `e` |
| `tab.merge_last` |  | merge the content's tab shown into the tab shown before it, or else the tab beside it: every window of it, as it is split, right of every window there, the focused one keeping the focus ([content](../content.md)) | `global`: `Ctrl+q M`, `Ctrl+q m m` |
| `tab.next` | Next Tab | show the next tab of the area the keys are in: the details panel's tabs there, elsewhere the content's tabs; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+Tab`, `Ctrl+PageDown`, `Cmd+}`, `Ctrl+q g t` |
| `tab.prev` | Previous Tab | show the previous tab of the area the keys are in: the details panel's tabs there, elsewhere the content's tabs; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+Shift+Tab`, `Ctrl+PageUp`, `Cmd+{`, `Ctrl+q g T` |
| `tab.last` | Tab Before | show the content's tab shown before the one shown now | `global`: `Ctrl+q g Tab` |
| `tab.move_prev` |  | move the content's tab shown one place toward the first; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+q g <`, `Ctrl+Shift+PageUp` |
| `tab.move_next` |  | move the content's tab shown one place toward the last; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+q g >`, `Ctrl+Shift+PageDown` |
| `cursor.down` |  | move the selection in the tree to the next row; in the details panel, in the list it shows, or scroll what else it shows | `tree`: `j`, `ArrowDown`<br>`details`: `j`, `ArrowDown` |
| `cursor.up` |  | move the selection in the tree to the previous row; in the details panel, in the list it shows, or scroll what else it shows | `tree`: `k`, `ArrowUp`<br>`details`: `k`, `ArrowUp` |
| `cursor.first` |  | move the selection in the tree to the first row; in the details panel, in the list it shows, or scroll what else it shows | `tree`: `g g`<br>`details`: `g g` |
| `cursor.last` |  | move the selection in the tree to the last row; in the details panel, in the list it shows, or scroll what else it shows | `tree`: `G`<br>`details`: `G` |
| `cursor.go` |  | open the selected session, shell, worktree or file: go to a window showing it, in any tab, or else open it in a new tab after the one shown, and move to it, or to the prompt of a session not started yet; on a folded row, a worktree or a session holding others too, unfold it instead; fold or unfold a workspace, a heading or a directory; in the details panel, open the selected row, an image of Resources as a file is opened, the keys staying in the panel | `tree`: `Enter`, `o`, `l`, `L`, `Shift+Enter`, `Ctrl+Enter` |
| `cursor.split` |  | open the selected session, shell, worktree or file as `cursor.go` does, in a new window split off the focused one, below it; in the details panel, the selected image of Resources | `tree`: `s` |
| `cursor.vsplit` |  | open the selected session, shell, worktree or file as `cursor.go` does, in a new window split off the focused one, beside it; in the details panel, the selected image of Resources | `tree`: `v` |
| `cursor.tab` |  | open the selected session, shell, worktree or file as `cursor.go` does, in a new tab after the one shown, though a window shows it already; in the details panel, the selected image of Resources | `tree`: `t` |
| `cursor.fold` |  | fold the selected node when it is open, or else the row it sits under, going there; in the details panel, go back out to the list what it shows was opened from | `tree`: `h`<br>`details`: `Escape`, `Backspace` |
| `cursor.close` |  | close every window showing the selected session, shell or Neovim, or the selected file in its viewer or its Neovim; what they showed runs on | `tree`: `x` |
| `cursor.copy` |  | copy the path of the selected file, worktree or workspace, or the name of the selected session, shell or Neovim; in the details panel, the selected link of the overview: its address, or its file's path | `tree`: `y`<br>`details`: `y` |
| `cursor.reveal` |  | in the details panel, show the selected file of the overview in the system's file manager |  |
| `cursor.browse` |  | open the selected pull request or issue on GitHub in the browser | `tree`: `g x`<br>`viewer`: `g x` |
| `filer.down` |  | move to the next file or directory, in the tree's files or the picker's directories | `filer`: `j`, `ArrowDown` |
| `filer.up` |  | move to the previous file or directory | `filer`: `k`, `ArrowUp` |
| `filer.into` |  | open the directory selected and go to its first entry once it is read; on a file of the tree, open it as `cursor.go` does | `filer`: `l`, `ArrowRight` |
| `filer.out` |  | close the directory selected when it is open, or else the one it is in, going there | `filer`: `h`, `ArrowLeft` |
| `filer.open` |  | in the tree, open the file or fold the directory selected as `cursor.go` does; in the picker, start a session in the directory selected | `filer`: `Enter` |
| `filer.hidden` |  | show or hide the files and directories whose names start with `.` | `filer`: `!` |
| `file.new` |  | in the tree's files, type the name of a file to make in the directory selected, in the one the file selected is in, or at the top of the worktree from its Files heading; a name with `/` makes the directories it passes through too, one ending in `/` makes a directory, and one already there is left as it is | `tree`: `N` |
| `directory.new` |  | type the name of a directory to make, where `file.new` makes a file | `tree`: `K` |
| `file.copy` |  | in the tree's files, put the file or directory selected on the system clipboard, to be copied where it is pasted | `tree`: `C` |
| `file.move` |  | put the file or directory selected on the system clipboard, to be moved where it is pasted | `tree`: `M` |
| `file.paste` |  | copy the files and directories on the system clipboard into the directory selected, the one the file selected is in, or the top of the worktree from its Files heading, as `file.new` makes there; move them, after asking, when `file.move` put them there; a name taken there already is left as it is | `tree`: `P` |
| `file.trash` |  | move the file or directory selected to the work machine's trash, after asking | `tree`: `D` |
| `file.mark` |  | mark the file or directory selected, or unmark it; `file.copy`, `file.move` and `file.trash` take every one marked rather than the one selected | `tree`: `-` |
| `paste.overwrite` |  | when a name pasted is taken, put the one there in the trash and paste in its place | `confirm`: `f` |
| `paste.rename` |  | when a name pasted is taken, type another name to paste it by | `confirm`: `r` |
| `waiting.next` | Next Waiting | open the next session waiting on you as `cursor.go` does, with the keys in its console; repeats ([keys](../keys.md#repeating)) | `global`: `Mod+j` |
| `waiting.prev` | Previous Waiting | open the previous session waiting on you as `cursor.go` does, with the keys in its console; repeats ([keys](../keys.md#repeating)) | `global`: `Mod+k` |
| `session.next` | Next Session | open the session after the one shown, in the tree's order over every session, folded away or filtered out too, going round at the end, as `cursor.go` does; the keys stay in the area they are in; repeats ([keys](../keys.md#repeating)) |  |
| `session.prev` | Previous Session | open the session before the one shown, in the tree's order over every session, folded away or filtered out too, going round at the start, as `cursor.go` does; the keys stay in the area they are in; repeats ([keys](../keys.md#repeating)) |  |
| `workspace.next` | Next Workspace | open a session of the workspace after the one shown, in the tree's order over every workspace holding a session, folded away or filtered out too, going round at the end: the session last shown there since the window opened, or else its first, as `cursor.go` does; the keys stay in the area they are in; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+q }`<br>`tree`: `}` |
| `workspace.prev` | Previous Workspace | open a session of the workspace before the one shown, as `workspace.next` does, going round at the start; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+q {`<br>`tree`: `{` |
| `session.pick` | Go to or Start Session… | open the picker, to go to a session or choose where a new one works ([details](../tree.md#going-to-a-session-or-starting-one)) | `global`: `Mod+p`<br>`tree`: `w` |
| `session.new` | New Session | start a session, with the prompt ready to write its first message: in the selected row's worktree or directory from the tree, in the one the focused window shows from elsewhere | `global`: `Mod+t`<br>`tree`: `n` |
| `session.scratch` | New Scratch Session | start a scratch session, with the prompt ready: one that runs in the scratch folder, for talking about something no project holds ([details](../tree.md#scratch-sessions)) |  |
| `session.resume` | Resume Conversation… | list the conversations Claude Code and Codex kept, to resume one in a new session ([details](../sessions.md#resuming-a-past-conversation)) | `global`: `Mod+o`<br>`tree`: `O` |
| `session.bring_back` | Bring Back Sessions… | list what the terminal host took with it when it went, to bring a session or a shell back ([details](../sessions.md#when-the-terminal-host-goes)) |  |
| `relay.review` | Review Words to Pass On… | read the words a session proposes to pass on to others as yours, to approve or decline them ([details](../opening-sessions.md#your-words-passed-on)) |  |
| `relay.decline` |  | pass on nothing of the words read | `confirm`: `d` |
| `session.stop` | End… | end a session Adroit runs after asking; Claude Code keeps its conversation, for `/resume` in a session started later; on a terminal's row, end that terminal | `global`: `Mod+Backspace`<br>`tree`: `X`<br>`console`: `X` |
| `session.rename` | Rename… | edit the name of a session Adroit runs in place; the tree shows it, and the session is sent Claude Code's `/rename` so its own interface says it too | `tree`: `R` |
| `session.profile_next` | Next Profile of New Session | while a session not started yet is shown, switch the profile it is to start from to the next one ([details](../sessions.md#agents)) | `draft`: `Ctrl+g p` |
| `session.profile_prev` | Previous Profile of New Session | while a session not started yet is shown, switch the profile it is to start from to the previous one ([details](../sessions.md#agents)) | `draft`: `Ctrl+g P` |
| `shell.open` | Open Shell | open a shell in the worktree | `global`: `Mod+s`<br>`tree`: `$` |
| `terminal.stop` | Stop Terminal | end the terminal selected in the tree, or else the one shown |  |
| `neovim.open` | Open Neovim | open Neovim, with your own configuration, in the worktree as `shell.open` opens a shell ([shells](../content.md#terminals)) | `global`: `Mod+e`<br>`tree`: `e` |
| `tab.close` | Close Tab | close the content's tab shown and every window in it; agents and terminals keep running and remain reachable from the tree | `global`: `Mod+w` |
| `environment.add` | Add Environment… | add an environment, on this computer or on a work machine paired then, which every window lists at once ([details](../environments.md)) |  |
| `environment.remove` |  | remove the environment selected in the tree from every window after asking, unless the window was started in it; its sessions keep running on its host ([details](../environments.md)) |  |
| `environment.rename` |  | edit the name of the environment selected in the tree in place, on its heading; every window calls it by the name given ([details](../environments.md)) |  |
| `environment.retry` |  | try at once to reach again the work machine of the environment selected in the tree, or else of the tab shown, while the window waits to try it ([details](../environments.md)) |  |
| `environment.restart_host` |  | have the work machine of the environment selected in the tree, or else of the tab shown, restart its work host, which ends no session ([details](../remote-execution.md#paired-work-machines)) |  |
| `environment.update` |  | after asking, have the work machine of the environment selected in the tree, or else of the tab shown, update its Adroit, which ends no session ([details](../remote-execution.md#paired-work-machines)) |  |
| `environment.pair` |  | pair again with the work machine of the environment selected in the tree, or else of the tab shown, as when it no longer accepts this computer or answers at another address ([details](../environments.md#create-and-configure)) |  |
| `workspace.add` | Add to Tree… | open the picker on the directories alone, to add the one chosen to the tree with nothing running there ([details](../tree.md#going-to-a-session-or-starting-one)) | `tree`: `A` |
| `workspace.remove` |  | take the workspace selected in the tree out of it, which lists it while nothing runs there too; where something runs, ask first and end every session and terminal running there. Starting anything there lists it again ([details](../tree.md#workspaces-and-worktrees)) |  |
| `worktree.close` |  | end, after asking, every session and terminal running in the worktree selected in the tree, which takes its row out of it; the workspace stays listed ([details](../tree.md#workspaces-and-worktrees)) |  |
| `window.close` | Close Window | close the window; the instance's other windows stay open, and its sessions run on | `global`: `Cmd+W` |
| `window.open` | New Window | open another window of the instance ([windows](../sessions.md#windows)) | `global`: `Mod+n` |
| `refresh` | Refresh | read the sessions, and what git says of where they run, again now | `global`: `Mod+r`<br>`tree`: `r` |
| `filter` | Filter… | narrow the tree to the sessions, shells, workspaces and worktrees whose names hold what is typed next; `Enter` keeps it, `Escape` clears it | `global`: `Mod+f`<br>`tree`: `/` |
| `details.tab` |  | show the details panel's tab numbered `n`, from 1 to 9, of those it has for what it describes ([details](../details.md#components)), and the panel when it is hidden | `details`: `1` (`details.tab 1`), `2` (`details.tab 2`), `3` (`details.tab 3`), `4` (`details.tab 4`), `5` (`details.tab 5`), `6` (`details.tab 6`), `7` (`details.tab 7`), `8` (`details.tab 8`), `9` (`details.tab 9`) |
| `worktrees.toggle` | Toggle All Worktrees | list every worktree git knows, those without sessions too, or only those with sessions again | `tree`: `!` |
| `scroll.down` |  | scroll the details panel down a step; bound in the console, scroll it: Claude Code and other programs that read the mouse are sent the wheel, others are scrolled back through what scrolled off; bound in a viewer, scroll what it shows | `viewer`: `j`, `ArrowDown` |
| `scroll.up` |  | scroll up a step, what `scroll.down` scrolls | `viewer`: `k`, `ArrowUp` |
| `scroll.half_down` |  | scroll down half a page, what `scroll.down` scrolls | `details`: `Ctrl+d`<br>`viewer`: `Ctrl+d`<br>`overlay`: `Ctrl+d` |
| `scroll.half_up` |  | scroll up half a page, what `scroll.down` scrolls | `details`: `Ctrl+u`<br>`viewer`: `Ctrl+u`<br>`overlay`: `Ctrl+u` |
| `scroll.top` |  | scroll to the top, what `scroll.down` scrolls | `viewer`: `g g` |
| `scroll.bottom` |  | scroll to the bottom, what `scroll.down` scrolls | `viewer`: `G` |
| `console.send_next` |  | send the next key as it is, whatever it is bound to | `console`: `Ctrl+g Ctrl+o` |
| `console.pass_every_key` |  | send every key as it is until `Escape`; `Ctrl+c` sends `Escape` meanwhile | `console`: `Ctrl+g i`, `Ctrl+g Ctrl+i` |
| `console.read` |  | read the console with Vim's keys, to move, search, select and yank in it, whatever it shows: a shell or a session showing a dialog too, whose keys are otherwise all its own; from a session's prompt, read the console above it | `global`: `Ctrl+q [` |
| `prompt.focus` | Prompt | give the prompt the keys |  |
| `prompt.write` |  | give the prompt the keys, and the key pressed with them | `tree`: `i`, `a`<br>`console`: `i`, `a`, `I`, `A`, `o`, `O` |
| `prompt.send` | Send Prompt | send what the prompt holds |  |
| `prompt.images` | Select Attached Image | select an attached image | `prompt`: `Mod+i` |
| `prompt.take_suggestion` |  | on an empty prompt, send the key pressed to the session and then bring the session's input into the prompt, so a prompt Claude Code suggests is taken to edit; an agent whose input cannot be taken back gets the key alone. On a prompt holding text or images, the key, Tab when none, is typed into Neovim |  |
| `prompt.history_prev` |  | put the prompt sent before the one shown in the prompt, to change and send again: those Claude Code recalls with its Up arrow where the focused window's session works | `console`: `ArrowUp`, `Ctrl+p` |
| `prompt.history_next` |  | put the prompt sent after the one shown in the prompt, as `prompt.history_prev` does | `console`: `ArrowDown`, `Ctrl+n` |
| `settings.open` | Settings… | open the Settings window, on the environment of the row selected in the tree or else of the tab shown | `global`: `Mod+,` |
| `daemon.restart` | Restart Session Daemon… | restart the sessions' daemon after asking, ending every session it holds ([details](../sessions.md#the-hosts)) |  |
| `keys.help` | Keyboard Shortcuts | show every binding in force, area by area, the areas the keys are in first, each with what its action does and whether it repeats; `Escape`, `q` or the key that showed it closes it, and no other key acts behind it | `global`: `F1`<br>`prompt`: `F1` |
| `zoom.in` | Zoom In | draw the window a step larger | `global`: `Mod+=`, `Mod+Plus` |
| `zoom.out` | Zoom Out | draw the window a step smaller | `global`: `Mod+-` |
| `zoom.reset` | Actual Size | draw the window at the zoom the settings give | `global`: `Mod+0` |
| `tree.toggle` | Toggle Tree | show or hide the tree | `global`: `Mod+b` |
| `details.toggle` | Toggle Details | show or hide the details panel | `global`: `Mod+d` |
| `viewer.zoom_in` |  | in a viewer, draw an image, a PDF, an HTML page or a Markdown document larger ([viewers](../content.md#viewers)) | `viewer`: `Plus`, `=` |
| `viewer.zoom_out` |  | in a viewer, draw an image, a PDF, an HTML page or a Markdown document smaller | `viewer`: `-` |
| `viewer.zoom_fit` |  | in a viewer, draw an image, a PDF, an HTML page or a Markdown document fitting the window | `viewer`: `0` |
| `viewer.page_next` |  | in a viewer, show a PDF's next page | `viewer`: `J` |
| `viewer.page_prev` |  | in a viewer, show a PDF's previous page | `viewer`: `K` |
| `viewer.play` |  | in a viewer, play or pause a movie | `viewer`: `Space` |
| `viewer.back` |  | in a viewer, seek a movie back | `viewer`: `h` |
| `viewer.forward` |  | in a viewer, seek a movie forward | `viewer`: `l` |
| `viewer.copy_path` |  | in a viewer, copy the path of the file it shows | `viewer`: `y` |
| `lightbox.next` |  | in the lightbox, show the next image of the session's Resources, going round at the end ([images in the console](../content.md#images-in-the-console)) | `lightbox`: `l`, `ArrowRight` |
| `lightbox.prev` |  | in the lightbox, show the previous image, going round at the start | `lightbox`: `h`, `ArrowLeft` |
| `lightbox.zoom_in` |  | in the lightbox, draw the image larger | `lightbox`: `Plus`, `=` |
| `lightbox.zoom_out` |  | in the lightbox, draw the image smaller | `lightbox`: `-` |
| `lightbox.zoom_fit` |  | in the lightbox, draw the image whole, in the middle | `lightbox`: `0` |
| `lightbox.pan_left` |  | in the lightbox, move the view of the image left | `lightbox`: `H` |
| `lightbox.pan_right` |  | in the lightbox, move the view of the image right | `lightbox`: `L` |
| `lightbox.pan_up` |  | in the lightbox, move the view of the image up | `lightbox`: `K` |
| `lightbox.pan_down` |  | in the lightbox, move the view of the image down | `lightbox`: `J` |
| `lightbox.close` |  | close the lightbox | `lightbox`: `Escape`, `q` |
| `overlay.close` |  | close what is open over the window: a picker, a menu, the keys help or the sheet adding an environment; a question is answered no ([keys](../keys.md#over-the-window)) | `overlay`: `Escape`, `Ctrl+c`, `q`<br>`confirm`: `n` |
| `overlay.down` |  | over the window, move to the next row of a picker's list, a menu or the sheet adding an environment, going round at the end; scroll the keys help down a line | `overlay`: `Ctrl+n`, `Ctrl+j`, `ArrowDown`, `Tab`, `j` |
| `overlay.up` |  | over the window, move to the previous row, as `overlay.down` moves to the next; scroll the keys help up a line | `overlay`: `Ctrl+p`, `Ctrl+k`, `ArrowUp`, `Shift+Tab`, `k` |
| `overlay.choose` |  | over the window, choose the row moved to in a picker's list, a menu or the sheet adding an environment; a question is answered yes | `overlay`: `Enter`<br>`confirm`: `y` |
| `console.send [<key>]` |  | send the key pressed to the session; with a key named after it, as in `console.send Escape`, send that key instead | `global`: `Ctrl+q .` (`console.send Ctrl+q`)<br>`console`: `Enter`, `Ctrl+Enter` (`console.send Enter`), `ArrowLeft`, `ArrowRight`, `Tab`, `Shift+Tab`, `F2`, `Ctrl+l`, `Ctrl+o`, `Ctrl+t`, `Ctrl+_`, `Ctrl+-` (`console.send Ctrl+_`), `Ctrl+c` (`console.send Escape`), `g Ctrl+c` (`console.send Ctrl+c`) |
| `tab.go <n>` |  | show the content's tab numbered `n`, from 1 to 8, or the last when there are fewer; 9 shows the last however many there are | `global`: `Ctrl+q 1` (`tab.go 1`), `Ctrl+q 2` (`tab.go 2`), `Ctrl+q 3` (`tab.go 3`), `Ctrl+q 4` (`tab.go 4`), `Ctrl+q 5` (`tab.go 5`), `Ctrl+q 6` (`tab.go 6`), `Ctrl+q 7` (`tab.go 7`), `Ctrl+q 8` (`tab.go 8`), `Ctrl+q 9` (`tab.go 9`), `Mod+1` (`tab.go 1`), `Mod+2` (`tab.go 2`), `Mod+3` (`tab.go 3`), `Mod+4` (`tab.go 4`), `Mod+5` (`tab.go 5`), `Mod+6` (`tab.go 6`), `Mod+7` (`tab.go 7`), `Mod+8` (`tab.go 8`), `Mod+9` (`tab.go 9`) |
| `tab.merge <n>` |  | merge the content's tab shown into the tab numbered `n`, as `tab.merge_last` does, from 1 to 8, or the last when there are fewer; 9 is the last however many there are | `global`: `Ctrl+q m 1` (`tab.merge 1`), `Ctrl+q m 2` (`tab.merge 2`), `Ctrl+q m 3` (`tab.merge 3`), `Ctrl+q m 4` (`tab.merge 4`), `Ctrl+q m 5` (`tab.merge 5`), `Ctrl+q m 6` (`tab.merge 6`), `Ctrl+q m 7` (`tab.merge 7`), `Ctrl+q m 8` (`tab.merge 8`), `Ctrl+q m 9` (`tab.merge 9`) |

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
"Ctrl+q e" = "window.edit"
"Ctrl+q m 1" = "tab.merge 1"
"Ctrl+q m 2" = "tab.merge 2"
"Ctrl+q m 3" = "tab.merge 3"
"Ctrl+q m 4" = "tab.merge 4"
"Ctrl+q m 5" = "tab.merge 5"
"Ctrl+q m 6" = "tab.merge 6"
"Ctrl+q m 7" = "tab.merge 7"
"Ctrl+q m 8" = "tab.merge 8"
"Ctrl+q m 9" = "tab.merge 9"
"Ctrl+q M" = "tab.merge_last"
"Ctrl+q m m" = "tab.merge_last"
"Ctrl+q [" = "console.read"
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
"Ctrl+q g <" = "tab.move_prev"
"Ctrl+q g >" = "tab.move_next"
"Ctrl+Shift+PageUp" = "tab.move_prev"
"Ctrl+Shift+PageDown" = "tab.move_next"
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
"l" = "cursor.go"
"s" = "cursor.split"
"v" = "cursor.vsplit"
"t" = "cursor.tab"
"L" = "cursor.go"
"Shift+Enter" = "cursor.go"
"Ctrl+Enter" = "cursor.go"
"h" = "cursor.fold"
"n" = "session.new"
"$" = "shell.open"
"e" = "neovim.open"
"O" = "session.resume"
"x" = "cursor.close"
"X" = "session.stop"
"R" = "session.rename"
"y" = "cursor.copy"
"g x" = "cursor.browse"
"i" = "prompt.write"
"a" = "prompt.write"
"r" = "refresh"
"/" = "filter"
"w" = "session.pick"
"A" = "workspace.add"
"!" = "worktrees.toggle"
"N" = "file.new"
"K" = "directory.new"
"C" = "file.copy"
"M" = "file.move"
"P" = "file.paste"
"D" = "file.trash"
"-" = "file.mark"
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
"1" = "details.tab 1"
"2" = "details.tab 2"
"3" = "details.tab 3"
"4" = "details.tab 4"
"5" = "details.tab 5"
"6" = "details.tab 6"
"7" = "details.tab 7"
"8" = "details.tab 8"
"9" = "details.tab 9"
"Ctrl+d" = "scroll.half_down"
"Ctrl+u" = "scroll.half_up"

[console]
"Enter" = "console.send"
"Ctrl+Enter" = "console.send Enter"
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

[console.claude]
"Tab" = "prompt.write"

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
"g x" = "cursor.browse"
"e" = "window.edit"
"q" = "window.quit"

[lightbox]
"l" = "lightbox.next"
"ArrowRight" = "lightbox.next"
"h" = "lightbox.prev"
"ArrowLeft" = "lightbox.prev"
"Plus" = "lightbox.zoom_in"
"=" = "lightbox.zoom_in"
"-" = "lightbox.zoom_out"
"0" = "lightbox.zoom_fit"
"H" = "lightbox.pan_left"
"L" = "lightbox.pan_right"
"K" = "lightbox.pan_up"
"J" = "lightbox.pan_down"
"Escape" = "lightbox.close"
"q" = "lightbox.close"

[overlay]
"Escape" = "overlay.close"
"Ctrl+c" = "overlay.close"
"q" = "overlay.close"
"Ctrl+n" = "overlay.down"
"Ctrl+j" = "overlay.down"
"ArrowDown" = "overlay.down"
"Tab" = "overlay.down"
"j" = "overlay.down"
"Ctrl+p" = "overlay.up"
"Ctrl+k" = "overlay.up"
"ArrowUp" = "overlay.up"
"Shift+Tab" = "overlay.up"
"k" = "overlay.up"
"Enter" = "overlay.choose"
"Ctrl+d" = "scroll.half_down"
"Ctrl+u" = "scroll.half_up"

[confirm]
"y" = "overlay.choose"
"n" = "overlay.close"
"f" = "paste.overwrite"
"r" = "paste.rename"
"d" = "relay.decline"
```

# Pass only control keys from the console to a session

Status: Accepted; superseded in part by
20261019-pass-every-key-while-claude-code-shows-a-dialog.md

## Context

The console passed every key to the session it showed, so text typed there
went into Claude Code's own prompt instead of the Neovim prompt the user writes
in. Claude Code still needs keys from the console: its menus and history are
driven by keys, and while it waits on a permission or a question, any key,
letters included, can be an answer.

## Decision

The console passes a session only the keys its controls use and sends the keys
that start writing to the prompt, except while the session reports it is
waiting on the user, when every key goes to it. Passing other keys at other
times takes an explicit request.

## Alternatives considered

- Keep passing every key, and only make clicking the console leave the
  keyboard with the prompt. Typing in the console would still write into Claude
  Code's prompt, which the user then has to notice and clear.
- Tell a choice from the input line by reading the session's screen. It breaks
  whenever the program redraws its interface differently, while the waiting
  state is something the session reports.

## Consequences

- Text for a session is written in Neovim, apart from answers to a choice.
- A choice the session does not report as waiting, such as a picker opened by
  a slash command, takes the explicit request for keys outside the passed set.
- Shells are terminals the user types into, so they keep taking every key.

# Concept

This document describes what adroit is for and the ideas every design decision
rests on. It is rewritten in place as the concept evolves; decisions built on it
are recorded as ADRs.

## What it is

adroit is a place to get things done with many coding agent sessions at once.

Its unit is a **session**: a conversation with an agent such as Claude Code or Codex. You start one by choosing
the directory it works in, its **workspace**, and talking. What the work turns
out to be shows as you go; nothing has to be named or declared before the
first word.

A workspace holds every session adroit runs in it. When the workspace is a git
repository, its worktrees divide it: each session sits under the worktree it
runs in. A session keeps its work in its own conversation: what can run
alongside, it gives to its agent's own subagents and tools, whose results come
back to it. When a separate line of work turns up, one that needs little of
what the session knows and that you will want to talk through yourself, the
session can open it as a session beside it, usually in a worktree of its own,
with a first prompt that carries what it needs. That session answers to you,
not to the one that opened it.

You see every workspace's sessions in one window, the ones waiting on you
first. You read each session
in the agent's real terminal interface, you write to it in your own Neovim,
and you move around the whole window the way you move around Vim.

## Who it is for

People who live in Vim and work with several coding agent sessions at the same
time, often across repositories, and who want that work to feel like editing
rather than like operating a dashboard.

## Principles

- **The agent interface stays as it is.** adroit shows each agent's own
  terminal interface unchanged. It never rebuilds an agent's screen from
  structured output.
- **Writing is Neovim.** Prompts are written in an embedded Neovim configured
  by the user for it, specialised for input rather than replaced.
- **Moving is Vim.** The window is navigated with Vim's grammar: window motions
  between panes, a tree browsed like a file explorer, actions on what is
  selected. Nothing requires the mouse.
- **Every session talks to you.** A session is one conversation that plans its
  work, does it and answers to you. Sessions stand beside one another: none
  reports to another or waits on another's result, and nothing stands between
  you and any of them. Work that has to come together is brought together by
  you, or by a session you ask to.
- **The session comes first.** Starting asks only where: a directory picked
  from those already in use, from the repositories on the machine, or from a
  file tree. Structure is never asked for up front.
- **Sessions outlive the window.** Every session runs in a terminal host, and
  everything known of the work is held by a work host, both apart from the
  window, so closing adroit leaves them running and opening it again finds
  them as they were. A window only shows the work, and any number of windows
  can show it.
- **adroit keeps no records of its own about sessions but which opened which
  and what they told each other.** Which sessions exist, where they run and
  how they stand come from the hosts that run them and from what their
  programs report; repositories and worktrees come from git. adroit draws them
  and gives you the controls.
- **Borrow before building.** Terminals and their persistence come from felis;
  conversations and the agents inside them come from Claude Code, Codex, Pi
  and OpenCode.
  adroit adds the place, the hands-on feel, and the means for sessions to
  open sessions beside them and reach one another.

## What it does not do

- It does not rebuild Claude Code's interface or replace it with a chat view.
- It does not orchestrate, and it is no frame for one session to run others
  as its workers. Each session plans and does its own work; adroit keeps no
  plan, task or verdict, shows where the work stands and gives you the
  controls.
- It is not an editor or an IDE. Files, logs and diffs are there to be read;
  real editing happens in your usual editor.

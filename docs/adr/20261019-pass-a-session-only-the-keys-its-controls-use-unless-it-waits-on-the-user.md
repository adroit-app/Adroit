# Pass a session only the keys its controls use, unless it waits on the user

Status: Accepted

## Context

Text for a session is written in the prompt. A console that passed every key
let typing land in the agent's own input line instead. Agents still need keys
from the console: menus and history are driven by keys, and while an agent
asks a question or shows a dialog, any key, letters included, can be the
answer.

## Decision

The console passes a session only the keys its controls use, and sends the
keys that start writing to the prompt. While the session waits on the user,
every key goes to it: when its program reports that it waits, or when its
adapter sees a dialog on its screen. Passing other keys at other times takes an
explicit request.

## Alternatives considered

- Pass every key, and only move the keyboard to the prompt on a click. Typing
  in the console would still write into the agent's input.
- Rely on the reported state alone. Agents report nothing for dialogs they open
  themselves, such as a picker opened by a slash command.
- List the commands that open a dialog. It misses dialogs opened otherwise and
  cannot tell when one closes.

## Consequences

- Seeing a dialog depends on how the agent draws it; when that changes, keys
  are held back again until the explicit request passes them.
- Shells are typed into, so they take every key.

# Give every session a short code that never changes

Status: Accepted

## Context

A session's name moves: it is made from the first words of the prompt, its
agent renames it once it knows the work and again when the work turns, and
the user renames it by hand. The id Adroit keeps for it is a UUID no one can
say or type, and it changes when a session is brought back after its terminal
host went, since the session brought back is recorded anew. The user had
nothing stable to call a session by, in the window, to another session or on
the command line.

## Decision

Every session carries a short code of a few letters and digits, given when it
is first recorded and never changed. No two sessions of an instance share a
code, and a code once given is never given again. A session brought back, or a
conversation resumed in a new session, keeps the code it had. The code is
shown beside the name wherever a session is listed, and wherever a session is
asked for by its id, its code is taken as well.

## Alternatives considered

- **Words, such as an adjective and a noun.** Easier to say, but longer to type
  and to fit beside a name in the tree and the tab, and a list of words has to
  be kept and reviewed for words that read badly.
- **A number counted up.** The shortest to type, but it says nothing and gives
  away how many sessions have run.
- **Making the name itself unchanging.** The name exists to say what the work
  is, and that changes; keeping it fixed undoes the agent's renaming.
- **A code that lasts only while its session runs.** A code that can come back
  for another session is no better than a name for referring to past work.

## Consequences

- Adroit keeps the code with each session's record, and hands it on to the
  record that brings the session back or resumes its conversation.
- Codes are drawn at random and checked against every code given before, so a
  code stays short only while the instance has given far fewer codes than the
  codes there are.
- A session can be told by its code from its record's id and from its name, so
  a code is never read as either.

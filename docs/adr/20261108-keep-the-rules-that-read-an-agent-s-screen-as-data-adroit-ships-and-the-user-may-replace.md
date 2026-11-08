# Keep the rules that read an agent's screen as data Adroit ships and the user may replace

Status: Accepted

## Context

When a program reports no status, Adroit reads its screen to tell whether it
works, waits on the user, has a dialog holding the keys, or is ready for its
first prompt. Agents redraw their screens often, and each change of wording
or layout broke a rule written in Rust, which only a new release of Adroit
could mend. The user who saw the breakage could not mend it.

## Decision

The rules that read an agent's screen — how it stands, whether a dialog holds
the keys, and whether it is ready at startup — are declarative data, one set
per agent, that Adroit carries inside itself. The user may put a set of their
own in place of an agent's whole set. A set of the user's that cannot be read
is ignored for the one Adroit carries, and Adroit says so where it explains a
judgement. Adroit fetches no rules from anywhere.

## Alternatives considered

- Keep the rules in Rust. Every change an agent makes to its screen waits for
  a release of Adroit.
- Let the user's set replace or add single rules by name. Which rules are in
  force, and in what order, could then be read from neither file alone.
- Fetch updated rules from a server, as Herdr does. Adroit reads every agent's
  screen and passes keys by what it reads; whoever controls the update path
  would steer that, and signing the rules is more than the gain is worth while
  a user can mend a rule themselves.

## Consequences

- The data needs an engine able to say what the Rust rules said, including
  regions found by lines such as an input box between two rules, and limits
  that refuse a set too large to evaluate on every read.
- A rule's name is the same in the data and in the explanation of a
  judgement, so the user can find the rule that misjudged.
- What cannot be judged, such as reading the channels a warning names, stays
  in Rust.
- A user's set outlives the release it was written for; it keeps judging until
  they remove it.

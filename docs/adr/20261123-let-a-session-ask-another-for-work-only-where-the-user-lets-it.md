# Let a session ask another for work only where the user lets it

Status: Accepted

## Context

A message between sessions hands over no work, so that sessions do not push
work onto each other, and one misled by what it read cannot set the others to
it. But what the user decides in one session is often for another to carry
out, and the user had to say it again there.

## Decision

A session asks another for work with a request, apart from a message. The
recipient takes it up as work only where the user lets that sender ask that
recipient, and is told it as information otherwise. A session that opened
another may ask it, as it already wrote what that session first set out to do;
any other pair, the opened asking its opener included, waits for the user to
let it. The work host judges it by the pair alone, as the request is handed
over, and only the user changes it. A request is not the user's own word: it
approves nothing and answers nothing that waits on the user.

## Alternatives considered

- Let every message hand over work. Any session could set any other to work,
  and one misled would mislead the rest.
- Let a session's opener and those it opened ask each other. A session set to
  read what others wrote could steer the one that knows and may do more, and
  the two could hand work back and forth.
- Let sessions in the same repository ask each other. Sessions with nothing to
  do with each other could, and those whose work spans repositories could not.
- Let the user approve each request. The user would step in as often as by
  saying it again.
- Refuse a request from a sender not let ask. The recipient would not hear of
  it, and the user would not see that it was asked.

## Consequences

- A session misled by what it read can set those it opened to work, as it
  already could by opening them.
- Whom the user lets ask is kept by the work host for as long as the sessions
  can be resumed.
- A recipient hears of work it is not to do, and spends a turn on it.
- What a request may lead to is bounded by what its recipient may already do,
  and by the agent asking the user before what cannot be taken back.

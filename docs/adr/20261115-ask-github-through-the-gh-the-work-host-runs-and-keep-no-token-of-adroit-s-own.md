# Ask GitHub through the gh the work host runs and keep no token of Adroit's own

Status: Accepted

## Context

A repository's open pull requests and issues are work waiting to be done in
it, and the tree is where the user goes back to their work. GitHub answers
only someone logged in for a private repository, and with a rate limit kept
per login for any. The repository lives on the work machine, which may not be
the computer the window runs on, and the user who has cloned it there has
usually logged in to GitHub's own CLI there already.

## Decision

The work host asks GitHub what a repository holds by running `gh` in it, and
sends the window what came back. Adroit stores, reads and asks for no token;
logging in, and choosing which login speaks for which repository, are left to
`gh`. Where `gh` is missing, Adroit asks GitHub nothing.

## Alternatives considered

- Call GitHub's API with a token Adroit keeps. Adroit would hold a secret,
  ask the user to make one, and follow its expiry, beside the login `gh`
  already keeps.
- Run `gh` from the window. The window may run on another computer than the
  repository, with another login or none, and it reaches the work only through
  the work host.
- Read the token `gh` keeps and call the API with it. Adroit would depend on
  where and how `gh` stores its login, which changes between releases, and
  would handle a secret all the same.

## Consequences

- What the tree shows of GitHub is what the login `gh` uses on the work
  machine can see.
- Each read costs a run of `gh` and a request against that login's rate
  limit, so the host reads only what a window shows.
- Adroit follows the shape of `gh`'s JSON output, which `gh` keeps stable but
  may extend, and of what GitHub's REST API answers through `gh api`.
- An image a private repository shows is served only to its login. The host
  asks `gh` for the text rendered as GitHub renders it, whose images are
  addresses GitHub signs for a few minutes, fetches them with no token and
  hands them to the window with the text.

# Refuse a model its provider does not list

Status: Accepted

## Context

A Leader names a Worker's model in prose it writes itself, and a misspelt or
retired name passed straight to the provider. Depending on the provider, the
Worker then exits at once or opens and stops at a model error, and the Leader
learns neither why nor that it could fix the name. Claude Code and Codex each
list the models they accept, without running a turn.

## Decision

Adroit keeps the list each provider gives of its models, and refuses a model
outside that list when the model is chosen for a managed session, before it is
recorded. While a provider has not given a list, its models are not checked.

## Alternatives considered

- Keep a table of model names in Adroit. It drifts with every release and does
  not know which models an account may use.
- Pass any name and report the provider's failure afterwards. The mistake is
  found only after a session has been started, and recognising each provider's
  failure screen is harder than asking for its list.
- Refuse every model while the list is unknown. A provider that fails to answer
  would stop every Worker that names a model.

## Consequences

- A Leader can correct a model while it is still planning the task.
- A model the provider accepts but does not list, such as a custom deployment,
  cannot be chosen per session; it can still be the configured default.
- Each provider is asked for its models from time to time, at the cost of a
  short-lived process.

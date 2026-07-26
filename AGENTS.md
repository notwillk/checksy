# Scope discipline

## Default rule

Make the smallest complete change that satisfies the user's literal request.

A request to move, rename, migrate, replace, or reorganize existing behavior is
a mechanical change unless the user explicitly asks for redesign, hardening, or
new infrastructure.

## Before editing

State a short scope contract containing:

- the requested outcome;
- the files expected to change;
- explicit non-goals; and
- the expected size of the diff.

Do not begin editing until this contract is internally consistent with the
request.

## Prohibited scope expansion

Unless explicitly requested or strictly necessary for correctness, do not add:

- dependencies;
- helper scripts;
- abstractions or frameworks;
- validators or test harnesses;
- new CI gates;
- refactors;
- hardening;
- unrelated fixes; or
- speculative future-facing infrastructure.

If any of these appear necessary, stop and explain why before implementing
them. Do not silently include them.

## Migration rule

For migrations, preserve behavior one-to-one:

1. Move the existing definitions.
2. Update direct callers.
3. Remove the superseded mechanism.
4. Run the existing validation.

Do not improve adjacent behavior during the migration. Report potential
improvements separately without implementing them.

## Diff budget

After the first implementation pass, run `git diff --stat`.

If the changed-file set exceeds the stated scope contract or the diff is
materially larger than expected, stop and reduce it before continuing. Do not
rationalize the larger diff after the fact.

## Commands

Run requested commands directly first. Do not invent wrappers or alternate
execution paths. If a command fails, report the actual failure before proposing
environment changes or escalation.

## Delegation

Subagents inherit the same scope contract. Do not delegate open-ended design or
"improvement" work during a mechanical migration.

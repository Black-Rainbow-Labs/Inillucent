@AGENTS.md

## Never put a ticket number in a published document

A ticket number such as `task-NNNN` names a card on a private board. Nobody reading this repository
or inillucent.com can look it up, so it tells them nothing. Do not write one in `README.md`,
`CHANGELOG.md`, `AGENTS.md`, anything under `docs/`, `agent-skills/` or `packaging/`, or a driver or
example readme. Say what the change did instead: "the change that sized a leaf's delta area by its
free space", not "task-NNNN". A commit hash or a date is fine. A path to a design document under
`tasks/` is a file name and may stay. `node tools/doc-facts/check.mjs` fails on any other ticket
number in those files. Commit messages, code comments and the design documents under `tasks/` are
not covered by this rule.

## Before `agent_done`: delete retired worktree evidence older than a day

When a ticket's worktree is retired, its `_agent_output` is moved into
`_agent_output/retired-worktrees/<task-N>/` in the main checkout. That evidence is kept for one day
after the retire, and nothing else deletes it. By 2026-10-28 the folder had grown to 226 GB, 147 GB of it one ticket's benchmark databases.
So the last step of every ticket here, just before `agent_done`, is:

```bash
node <ai-service checkout>/scripts/clean-retired-worktrees.cjs <your worktree or the main checkout>
```

The ai-service checkout is the one beside this repository's main checkout.

It deletes only the entries directly inside `retired-worktrees` whose folder was made more than 24
hours ago, prints what it deleted and the total freed, and exits 0 even when one entry cannot be
deleted. Put its `freed N GB` line in the closing comment. `--dry-run` lists what it would delete.
Do not write multi-gigabyte scratch databases into `_agent_output` and leave them there: delete them
before the ticket finishes, and keep the numbers in a results file.

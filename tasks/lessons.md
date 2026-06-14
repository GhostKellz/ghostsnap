# Lessons

## 2026-04-08

- When the user asks for a spec related to another project in the context of the current repo, confirm whether the file belongs in the current repo's `tasks/` directory before writing it elsewhere.
- If a requested document is meant to guide integration work for Ghostsnap, place it under `/data/projects/ghostsnap/tasks/`, even if the source project being analyzed lives in another repository.

## 2026-04-16

- When reviewing or rewriting `tasks/todo.md`, keep it focused on concrete implementation and product correctness work. Do not pad it with CI, workflow, or release-process items unless the user explicitly asks for those.
- When the user clarifies product direction mid-stream, reframe `tasks/todo.md` around the new target immediately instead of continuing to optimize stale branch priorities.
- If an integration is no longer a product pillar, move it out of the main product surface and treat it as secondary workflow documentation unless the user explicitly wants a dedicated CLI feature.

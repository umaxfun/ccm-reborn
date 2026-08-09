# Repository-wide code review

Verdict: **clean — no blocker or P1 finding remains.**

This is a full-repository review, not a diff review. It followed the
user-supplied `code-review2.md` checklist with independent security,
reliability/data-integrity, and quality/UX passes; every blocker/P1 found in
the first pass was fixed and reviewed again against the current worktree.

## Resolved findings and evidence

| Area | Final control | Regression/evidence |
| --- | --- | --- |
| Desktop boundary | Tauri mutation commands require the canonical, exact SC2 root and runtime markers; the fixture-friendly core/CLI stays explicit. | `require_desktop_game_root`; `make check`. |
| Game/profile path safety | Game targets, `Mods`, `.ccm-reborn`, selected profile `Banks`/`Saves`, and account-store roots reject symlinks or invalid state before mutation. | `install_rejects_a_symlinked_game_ancestor_before_mutation`; state validation regressions. |
| Shared `Mods` | A dependency baseline is retained while any slot owns it; normal restore preserves another slot’s cache, final restore returns the original baseline, and failed install rollback restores exact pre-operation bytes. | `repair_replaces_a_shared_mod_dependency_without_erasing_another_slot`. |
| State/recovery | State is constrained to exactly one campaign target, target or `Mods` files only, and a contained backup path. Inspection is read-only; mutation holds an OS-released lock. | malformed target/backup tests; `operation_lock_rejects_a_second_writer_for_the_same_game`. |
| Account isolation | Snapshot roots are namespaced by the canonical selected SC2 account. Ambiguous legacy global snapshots fail closed rather than silently crossing accounts. | account-root validation in profile transition. |
| Remote packages | HTTPS redirects are disabled, planning stages the same SHA-verified archive model used by Apply, and the catalog’s declared size is checked. | planner/install checks and bounded archive handling. |
| Restore contract | UI restore is per-slot; CLI requires `--target`. It never infers a newest slot. | CLI restore regression. |
| Cross-platform | Atomic replacement uses Windows `MoveFileExW` and standard rename elsewhere; CI covers macOS and Windows. macOS launch opens the selected `StarCraft II.app`. | `npm run tauri build -- --debug --bundles app`; CI workflow. |
| Source hygiene | Checked source files are capped at 500 physical lines. | `npm run check:source-size`. |

## Verification run after the final fix

```text
npm run check:source-size  PASS
npm run build              PASS
cargo test                 PASS (26 library + 4 CLI)
npm run tauri build -- --debug --bundles app  PASS (macOS .app)
git diff --check           PASS
```

The future family/major identity and progress-summary work remains explicitly
marked as roadmap in `design/progress-profiles-plan.md`; it is not represented
as an implemented runtime guarantee.

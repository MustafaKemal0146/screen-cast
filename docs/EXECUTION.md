# screencast — Autonomous Multi-Agent Execution Protocol (v2.0)

> This repository is structured for **single-agent** or **parallel multi-agent** development.
> Modular `<= 350`-line files and strict single-crate ownership keep each agent's context small and
> eliminate merge conflicts.

---

## 1. Golden rules

1. **Contract stability (`D-010`).** `screencast-core` holds shared types, traits, config, and the `en`/`tr` i18n catalog. Sibling crates depend strictly one-way on `screencast-core`.
2. **Single-role file ownership.** Each role edits only its owned directories. Workspace-wide files (`Cargo.toml`, `README.md`, `docs/`) belong exclusively to Role **H**.
3. **350-line file cap (`D-011`).** Every `.rs` non-test section and every JS/CSS/HTML/Python asset must stay `<= 350` lines. Split growing files into `module/<concern>.rs` submodules.
4. **Zero Turkish outside dedicated catalogs.** All code identifiers, comments, logs, and docs are English. User-facing Turkish strings belong strictly in `screencast-core/src/i18n/tr/`, `assets/player/i18n_tr.js`, and `assets/gui/screencast_gui_i18n_tr.py`.
5. **Green gates before handoff:**
   - `cargo fmt --all -- --check`
   - `cargo clippy --workspace -- -D warnings`
   - `cargo test --workspace` (including `ai_hygiene` and `i18n_cli`)
   - `python3 -m py_compile assets/gui/*.py`
6. **Commit discipline:** Conventional Commits with Task ID: `feat(sinks): cast handshake [T-M4-02]`.

## 2. Recommended agent concurrency

| Agent count | Pattern | Best for |
|---|---|---|
| 1 | Sequential (`core -> transport/capture/sinks/ui -> cli/docs`) | End-to-end refactors & verification |
| 4 (Recommended) | Domain parallel: **A** (`transport`), **B** (`capture`+`encode`), **C+D** (`audio`+`sinks`), **E+F** (`ui`+`assets`), integrated by **H** | Fastest conflict-free delivery |

## 3. Roles & file ownership

| Role | Crate / Scope | Owned paths |
|---|---|---|
| **A** | `screencast-transport` | `crates/screencast-transport/**` |
| **B** | `screencast-capture`, `screencast-encode` | `crates/screencast-{capture,encode}/**` |
| **C** | `screencast-audio` | `crates/screencast-audio/**` |
| **D** | `screencast-sinks` | `crates/screencast-sinks/**` |
| **E** | `screencast-ui` (TUI & Rust wrappers) | `crates/screencast-ui/**` |
| **F** | Web player & GTK3 desktop GUI | `assets/player/**`, `assets/gui/**` |
| **G** | `screencast-core` (Contracts & i18n catalog) | `crates/screencast-core/**` |
| **H** | CLI, integration tests, packaging, docs | `crates/screencast-cli/**`, `docs/**`, `packaging/**`, `.github/**`, `Cargo.toml`, `README.md`, `AGENTS.md` |

## 4. Verification gates across waves

| Gate | Requirement |
|---|---|
| Workspace build | `cargo build --workspace` compiles cleanly with 0 warnings |
| Unit & integration | `cargo test --workspace` passes (all loopback network tests green, live hardware tests `#[ignore]`) |
| AI-hygiene gate | `cargo test -p screencast-cli --test ai_hygiene` passes (`<= 350` lines, clean English codebase, strict 1-way DAG) |
| i18n CLI gate | `cargo test -p screencast-cli --test i18n_cli` passes (`en` default + `--lang tr` + `SCREENCAST_LANG` + `config.toml` + `selftest`) |
| Packaging lint | `bash -n packaging/deb/build-deb.sh` and `python3 -m py_compile assets/gui/*.py` pass |

## 5. Subagent prompt template

```
Implement task <T-Mx-yy> in screen-cast. Read AGENTS.md, docs/PLAN.md (<T-Mx-yy>),
and the matching section in docs/ARCHITECTURE.md or docs/PROTOCOLS.md first.
Constraints:
- Modify only your role's owned paths (`docs/EXECUTION.md` §3).
- Keep every Rust non-test file and every asset <= 350 lines.
- Route all user-facing messages through `screencast_core::i18n` (`en` default, `tr` catalog).
- Run `cargo fmt --all -- --check`, `cargo clippy --workspace -- -D warnings`, and `cargo test --workspace`.
Report changed files, test output, and any untested edge cases.
```

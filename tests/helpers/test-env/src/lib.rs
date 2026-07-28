//! Pre-`main` hermetic environment floor for worktrunk's test binaries.
//!
//! Linking this crate installs, before `main` runs, the environment that makes
//! the suite independent of the developer's git configuration: the deny pair
//! pointing `GIT_CONFIG_GLOBAL`/`GIT_CONFIG_SYSTEM` at a path that does not
//! exist, the two settings the suite needs in the denied config's place, and
//! the `COLUMNS` width unit tests measure against. Every test target activates
//! it with one line in its crate root:
//!
//! ```ignore
//! use wt_test_env as _;
//! ```
//!
//! `assert_hermetic_floor` in `worktrunk::testing` fails fixture construction
//! with that instruction when a target forgets the line.
//!
//! # Why before `main`, and why in the binary
//!
//! An in-process test drives git through the test process's own environment —
//! `Repository::run_command` builds a plain `Cmd::new("git")`, so there is no
//! per-command hook a fixture can reach. A test cannot set that environment
//! for itself: under `cargo test`, tests are parallel threads in one process,
//! and `std::env::set_var` while another thread spawns a process (which reads
//! `environ`) is undefined behavior — the reason `set_var` is `unsafe`. Before
//! `main` the process has exactly one thread, so a constructor is the one
//! place the write is sound.
//!
//! Living in the binary rather than the runner means every way of running a
//! test agrees by construction: `cargo test`, nextest, `cargo llvm-cov`,
//! `cargo bench`, the Nix derivation, an IDE, a debugger, and a directly
//! executed `target/debug/deps/integration-*` all get the same floor, because
//! the binary carries it. A runner-level floor (cargo's `[env]`) covers only
//! processes cargo itself starts — and taxes `cargo run`, where a developer's
//! own `wt` invocation should see their real config and identity.
//!
//! Without the floor the suite reads whatever the developer configured —
//! `commit.gpgsign` fails every fixture commit, `core.hooksPath` runs their
//! hooks, `core.fsmonitor` and `credential.helper` run programs of their
//! choosing — and a conditional `includeIf "gitdir:…"` makes which of those
//! happens depend on where the fixture root sits.
//!
//! # What the floor sets
//!
//! The deny pair names a path that does not exist, in the same directory as
//! `DEFAULT_ISOLATED_USER_CONFIG` names for worktrunk's own config. Git reads
//! a missing config file as an empty one; only an explicit `--global` /
//! `--system` read of it fails, and nothing in `wt` performs one.
//!
//! `GIT_CONFIG_COUNT` + `GIT_CONFIG_KEY_n`/`GIT_CONFIG_VALUE_n` are git's
//! environment spelling of `-c`, so they apply wherever the deny pair does.
//! Two settings, both with work to do once the host's config is denied:
//!
//! - `user.useConfigOnly` stops git *guessing* an identity from the OS
//!   username and hostname when none is configured — the one way a hermetic
//!   suite could still author a commit as the developer. Nothing exercises it
//!   (every path sets an identity), so it is a backstop, kept because without
//!   it a future gap goes silent instead of failing. Identity itself comes
//!   from `git_test_env` (harness-built git) and `LOCAL_TEST_CONFIG`
//!   (in-process git); an entry here would be a second mechanism.
//! - `rerere.enabled = false` is set rather than left unset because git turns
//!   rerere on by itself whenever `$GIT_DIR/rr-cache` exists. The cached
//!   standard fixture under `target/` is built once and copied per test, so a
//!   single rebase during its construction leaves an `rr-cache` behind that
//!   would silently enable rerere for every test copying it — and only on
//!   machines whose cache predates the change. Pinning it false makes the
//!   suite's rerere state independent of what a fixture happens to carry.
//!
//! The `-c` spelling also sets precedence: it outranks a repository's own
//! config, where a global *file* would yield to it. So a key belongs here only
//! when no test needs to override it locally — which is why
//! `init.defaultBranch` does not, and every `git init` in the harness names
//! its branch instead: `default_branch.rs` sets that key in a repo to prove
//! `wt` reads it, and an entry here would silently win.
//!
//! `COLUMNS = 80` pins the width unit tests observe through
//! `terminal_width()` in-process; integration tests override it per command.

/// The floor, as data, so the guard in `worktrunk::testing` and the constructor
/// below cannot drift apart.
pub const FLOOR: [(&str, &str); 8] = [
    ("GIT_CONFIG_GLOBAL", "/nonexistent/wt/gitconfig"),
    ("GIT_CONFIG_SYSTEM", "/nonexistent/wt/gitconfig"),
    ("GIT_CONFIG_COUNT", "2"),
    ("GIT_CONFIG_KEY_0", "user.useConfigOnly"),
    ("GIT_CONFIG_VALUE_0", "true"),
    ("GIT_CONFIG_KEY_1", "rerere.enabled"),
    ("GIT_CONFIG_VALUE_1", "false"),
    ("COLUMNS", "80"),
];

/// Installs [`FLOOR`] into the process environment, overwriting any inherited
/// values so an exported variable can't reinstate the host's config.
#[ctor::ctor(unsafe)]
fn install_floor() {
    for (key, value) in FLOOR {
        // SAFETY: this runs before `main`, where the process has exactly one
        // thread — nothing can be reading the environment concurrently.
        unsafe { std::env::set_var(key, value) };
    }
}

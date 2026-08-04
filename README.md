<div align="center">

# nezuko

**a mini-tokio you can read in an afternoon**

[![ci](https://github.com/kuramalabs/nezuko/actions/workflows/ci.yml/badge.svg)](https://github.com/kuramalabs/nezuko/actions/workflows/ci.yml)
[![audit](https://github.com/kuramalabs/nezuko/actions/workflows/audit.yml/badge.svg)](https://github.com/kuramalabs/nezuko/actions/workflows/audit.yml)
[![crates.io](https://img.shields.io/crates/v/nezuko.svg)](https://crates.io/crates/nezuko)
[![docs.rs](https://img.shields.io/docsrs/nezuko)](https://docs.rs/nezuko)
[![license](https://img.shields.io/crates/l/nezuko.svg)](#license)
[![msrv](https://img.shields.io/badge/msrv-1.95-blue)](#msrv)

</div>

---

`nezuko` is a small async runtime built to be understood. The goal is not to
compete with `tokio` - it is to make every layer of a runtime (task, waker,
executor, reactor) small enough to fit in your head.

If you have wondered:

- Where exactly does a `Waker` come from?
- What does the reactor actually store when it registers an `fd`?
- How does `spawn` return before the future runs?

Read the source.

## Quick start

```rust
use nezuko::Runtime;

fn main() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        println!("hello from nezuko");
    });
}
```

## Layout

```
src/
├── lib.rs        # public surface
├── runtime.rs    # Runtime handle, block_on, spawn
├── task.rs       # Task, JoinHandle, waker vtable
├── reactor.rs    # mio-backed I/O driver
└── sync.rs       # oneshot, mpsc, Notify
```

## Development

Everything goes through `make`. First time on a machine:

```bash
make bootstrap        # installs tools + wires .githooks/
make setup
```

Day-to-day:

| command         | what it does                                     |
| --------------- | ------------------------------------------------ |
| `make watch`    | rerun `check` + `nextest` on every save          |
| `make fmt`      | format the tree                                  |
| `make clippy`   | lint with `-D warnings`                          |
| `make test`     | nextest                                          |
| `make coverage` | HTML report at `target/llvm-cov/html/index.html` |
| `make bench`    | Criterion, HTML report at `target/criterion`     |
| `make ci`       | run the full CI gate locally                     |

Security:

| command         | what it does                                    |
| --------------- | ----------------------------------------------- |
| `make audit`    | RustSec advisory scan of `Cargo.lock`           |
| `make deny`     | advisories + licenses + banned crates + sources |
| `make outdated` | dependencies with newer versions available      |
| `make machete`  | unused dependencies                             |

## Docker

Multi-stage build with `cargo-chef` for dep caching and `sccache` for
compiler-artifact caching, ending in a distroless runtime.

```bash
make docker-build
make docker-run
```

## Git hooks

Hooks live in `.githooks/` (version-controlled). `make bootstrap` — or
`make hooks` on its own — points `core.hooksPath` at that directory.

- **pre-commit** — `cargo fmt --check` + `cargo check` (fast).
- **pre-push** — `clippy -D warnings` + `nextest` + doc-tests.
- **commit-msg** — Conventional Commits.

Bypass pre-push in a pinch: `NEZUKO_SKIP_PREPUSH=1 git push`.

## CI

Three workflows under `.github/workflows/`:

- **ci.yml** — fmt, clippy, matrix test (Linux/macOS/Windows × stable/beta/MSRV),
  docs. A single `gate` job aggregates results so branch protection needs one
  required check.
- **audit.yml** — `cargo audit` and `cargo deny` (advisories/bans/licenses/sources
  sharded into a matrix). Runs on push, PR, and daily cron.
- **release.yml** — tag-triggered publish to crates.io + GitHub release with
  auto-generated notes.

Caching is via `Swatinem/rust-cache@v2`, test runner is `cargo-nextest`.

## MSRV

The minimum supported Rust version is **1.95**, enforced in CI. Bumping the
MSRV is a minor-version change.

## License

- MIT license ([`LICENSE-MIT`](LICENSE-MIT))

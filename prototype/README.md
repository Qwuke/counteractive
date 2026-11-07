# Counteractive prototype

A laptop-sized build of the system in [`../ARCHITECTURE.md`](../ARCHITECTURE.md). A wallet buys anonymous credit from a storefront, then spends it, one session at a time, at an attested gate that meters an inference engine. Every service runs as its own process. [`SPEC.md`](SPEC.md) is the contract each crate is built against. Where the prototype departs from SPEC, the departure is listed under [Deviations from SPEC](#deviations-from-spec).

The prototype runs on one machine. It replaces only what a laptop can't provide, and says so loudly. This table is from SPEC:

| Real system | Prototype stand-in | Same code above it? |
|---|---|---|
| SEV-SNP report and GPU evidence | `dev-insecure` platform: an Ed25519 signature over the same `REPORT_DATA` | Yes: binding, role, release selection, TLS checks |
| vLLM on a GPU | `fake-engine`, an OpenAI-compatible server with vLLM's `/tokenize` shape | Yes: the `openai` backend adapter |
| Stripe API | `fake-stripe`, the two Checkout Session endpoints plus a test pay call | Yes: the storefront's HTTP client, pointed at another base URL |
| Tor | Tier 1: the wallet dials the gate's Unix socket directly. Tier 2: C tor onion service plus Arti | Yes: the same TLS, attestation and HTTP code over a different `Dialer` |
| Compiled-in wallet pins | `wallet.toml`, written by `xtask` | Read through one `Pins` type, so a release build can swap in constants |
| Measured image, systemd, firewall | Processes started by `xtask` | No |

The prototype leaves out drain, export, import, SNP verification, real GPU appraisal, the boot collector, the image, release signing CI, proof-of-work and the OS keychain.

Every connection to the gate or the storefront prints a `dev-insecure` warning, because no hardware protects it.

## Crates

| Crate | Job |
|---|---|
| `proto` | Wire types: gate, storefront, walletd, the Stripe subset, the OpenAI subset, SSE, receipts and errors |
| `credit` | The only crate that names `act` types: the issuer (`Moderator`) and the client (`Holder`) |
| `manifest` | Manifest and `release.json` types, canonical JSON, minisign and the accept rule |
| `attest` | Report binding, the dev-insecure source and verifier, and the `snp` placeholder |
| `metering` | Holds, the one-debit rule, receipts and the wallet-side arithmetic |
| `crash-point` | `crash_point!` for kill tests, compiled out unless enabled |
| `gate-conn` | Dialers (Unix, TCP, Arti), inner TLS, `UnverifiedConn` and `AttestedConn`, the verdict cache and the server half |
| `backend` | The inference backend adapter |
| `gate` | The gate CVM service: unlock, issuer, sessions, metering, refunds and recover |
| `storefront` | The storefront CVM service: orders, Stripe Checkout and collect-time issuance through the gate |
| `wallet-core` | The journal (SQLCipher), the chain state machine, pins, the manifest store and reconcile |
| `wallet-cli` | Buying, over the TCP dialer only. It reaches the use-plane commands through `walletd` |
| `walletd` | The local wallet daemon: an OpenAI-compatible API, the wallet API and the web page, over the Unix or Arti dialer only |
| `fake-engine`, `fake-stripe` | The laptop stand-ins |
| `xtask` | The dev tree, process control and the end-to-end acceptance run |

## Build and test

Run these from `prototype/`:

```sh
cargo build --workspace
cargo test --workspace
cargo build --workspace --release
cargo test -p crash-point --features enabled
cargo check -p walletd --features arti     # walletd's tier-2 dialer
cargo run -p xtask -- e2e                  # SPEC section 10, tier 1, then the kill tests
cargo run -p xtask -- demo --check --local # the browser demo's API, headless
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p walletd --all-targets --features arti -- -D warnings
cargo fmt --all --check
```

`.dev/` (written by `xtask`) and `target*/` are git-ignored.

### Lints

The lint set is `[workspace.lints]` in `Cargo.toml`, and every crate
inherits it. Clippy runs with `-D warnings`, so the count stays at zero.

- **Pedantic, minus four.** `clippy::pedantic` is on as a group, apart
  from `missing_errors_doc`, `missing_panics_doc`, `must_use_candidate` and
  `similar_names`, which here are mostly noise.
- **Restriction lints, picked one by one.** Outside tests there is no
  `unwrap`, `panic!`, `todo!`, `dbg!` or string slicing (`clippy.toml`
  exempts tests).
- **Exceptions need a reason.** `allow_attributes` makes every exception
  an `#[expect(..., reason = "...")]`. That has to give a reason, and it
  fails the build once the warning it covers is gone. There are eleven:
  - six are integration-test roots, which let their setup helpers unwrap;
  - the rest are in `gate/src/lib.rs`, `gate-conn/src/target.rs`,
    `walletd/src/token.rs`, `walletd/src/dialer.rs` and
    `backend/src/stream.rs`, each with its reason beside it.
- **Copies are visible.** `clone_on_ref_ptr` makes every reference-count
  bump read `Arc::clone(&x)`, so a bare `.clone()` is always a real copy.
- **axum's parameter names are exempt.** `renamed_function_params` keeps
  trait impls on the trait's parameter names, except axum's extractors
  (`clippy.toml`). Their `req`, `parts` and `state` say less than ours.

These lints were counted in a copy of the tree and not adopted:

| Lint | Hits | Why not |
|---|---:|---|
| `unreachable_pub` | 402 | Mostly churn in binary crates, and it fights `redundant_pub_crate` |
| `arithmetic_side_effects` | 86 | Mostly counters; the metering arithmetic already saturates or clamps |
| `map_err_ignore` | 64 | Mostly deliberate: crypto and parse errors aren't passed on |
| `expect_used` | 14 | Each `expect` states an invariant |
| `wildcard_enum_match_arm` | 13 | The two on our own enums that mattered now name every variant |
| `cognitive_complexity` | 2 | The gate's turn loop (`sessions/chat.rs`, 29 against 25) and close (`sessions/close.rs`, 26), left for review |

**The first pass** changed 91 files, adding 326 lines and removing 270,
counted against a snapshot taken before it.

Most of it came from `cargo clippy --fix`:
- backticks in doc comments;
- `assert!(r.is_ok())` became `r.unwrap()`, so a failure prints the error;
- underscores in long literals;
- `Arc::clone`.

These were done by hand:
- **Exhaustive matches.** walletd's error-to-status map and the wallet's
  balance tally now name every variant. A new error or chain state can't
  fall silently into a default.
- **The SSE parser.** `proto`'s parser lost a `Result` that could never be
  `Err`.
- **The fake engine.** Its script parser uses `split_once`, not byte
  offsets.
- **Dependencies.** 32 entries changed:
  - 19 unused ones were removed;
  - 13 used only by tests moved to `[dev-dependencies]`.

  rustc's `unused_crate_dependencies` found them. It ran once and wasn't
  kept on, because it misfires when a crate has both a library and a
  binary.

## Running it

### The xtask commands

`xtask` writes a dev tree in `prototype/.dev/` by default. Pass `--dev-dir <path>` to use another one.

| Command | What it does |
|---|---|
| `keygen` | Writes `keys/`: K (the gate's store key), the operator's minisign key pair, the dev attestation key and the storefront operator token. It refuses to overwrite existing keys. |
| `release` | Writes `release.json`: products `usd-5` (500 cents, 5,000,000 credits) and `usd-1` (100 cents, 1,000,000 credits); models `fake-small` (in 1, out 3) and `fake-large` (in 4, out 12). |
| `up [--real-stripe]` | Builds the binaries. Starts fake-engine, fake-stripe, the gate and the storefront in the background, each in its own process group. Logs go to `.dev/logs/<name>.log`, PIDs to `.dev/up.json`, and the wallet pins to `.dev/wallet/wallet.toml`. TCP services take OS-assigned ports and print them. |
| `unlock` | Attests the gate and unlocks it with K. On the first boot it signs the gate's new issuer key into the manifest and posts it. Then it attests the storefront and gives it its Stripe key. |
| `down` | Stops the PIDs in `up.json`, newest first: SIGTERM, then SIGKILL after 15 s. It checks each PID still runs the same program before signalling it. |
| `e2e [--tor]` | The acceptance run in a fresh temp dir. See [End-to-end results](#end-to-end-results). |
| `demo [--local] [--check] [--port N] [--keep] [--no-browser]` | The whole system in a fresh temp dir, driven from a browser page. See [The browser demo](#the-browser-demo). |
| `demo --remote` | The same console against the stack on a GPU instance. See [On a rented GPU](#on-a-rented-gpu-xtask-remote). |
| `remote init\|deploy\|unlock\|tunnel\|status\|down` | Runs the stack on a rented GPU instance, driven from here. Its tree is `prototype/.verda/`. |
| `host` | Run by `deploy/verda/start.sh` on the instance: the stack around the engine on `run/engine.sock`, with C tor. |

### The browser demo

```sh
cargo run -p xtask -- demo              # over Tor; opens http://127.0.0.1:7700/
cargo run -p xtask -- demo --local      # no Tor: walletd reaches the gate on its Unix socket
cargo run -p xtask -- demo --check      # the same steps, headless, every number checked; exits 0 or 1
```

`demo` brings up the e2e stack in a fresh temp dir under `/tmp`. That is the operator's keys, a signed release, fake-engine, fake-stripe, the gate and the storefront, attested and unlocked, plus C tor publishing the gate as an onion service. Then it starts walletd with its Arti dialer and serves a console page on `127.0.0.1:7700`. The page walks through the system in four cards:

0. **Bring-up.** Each step as it happens, with C tor's bootstrap on one live line. Over Tor this usually takes about 20 s. Once it took 213 s, because the gate's new onion service took minutes to answer after C tor had bootstrapped. While xtask waits for it, the log says every 30 s what its probes get.
1. **Approve the manifest.** It runs `wallet-cli manifest fetch` (from the storefront, on the purchase plane) and then `manifest approve`, and shows the commands and their output. After approval, walletd fetches the gate's own manifest over the use plane, through Arti and the onion service. The page shows whether it is byte for byte the one approved, and how long the first Tor path took.
2. **Buy credit.** It runs `wallet-cli buy --product usd-1 --no-browser` and links to fake-stripe's checkout page in a new tab. Pressing **Pay** there sends you back to the console's `/paid`. The console runs `wallet-cli collect` every 2 s while an order is unpaid, and at once after `/paid`. When the credential is unblinded, the order shows the chain it became.
3. **Chat over Tor.** The page talks to walletd's OpenAI-compatible API. Replies stream token by token, because fake-engine runs with `--token-delay-ms 40`. Each turn ends with its receipt: tokens in and out, credits charged, what the session has left, and time to the first token. **End session & settle** closes the session and reconciles. The settlement shows s − t against the receipts, and the next message opens a new, unlinkable session.

Two strips across the top light up the hops each action crosses on the purchase plane and on the use plane. The sidebar shows the wallet's balance and chains, the components, and an activity log. The log has every command the console ran, the bootstrap lines of C tor and walletd, and every service's warnings, each dev-insecure warning shown once.

Over Tor, every session dials in its own Arti isolation scope. So the first message of each conversation waits for a fresh path to the onion service. That took 5 to 50 s in these runs, and the page counts the seconds while it waits. Later turns in the same session take about 1 s.

The console never gives the page walletd's token, K, or any operator key. The page holds a fresh per-run console token. The console checks `Host` and that token on `/api/*`, and forwards only the six walletd routes the page uses, adding walletd's token itself. Its CSP allows only its own origin and no inline code. `--keep` keeps the temp dir with every service's state and log. Ctrl-C stops every service.

### On a rented GPU: `xtask remote`

```sh
cargo run -p xtask -- remote init                          # keys, a release serving qwen3-4b, an SSH key, in .verda/
# create the instance with .verda/ssh/id_ed25519.pub as its SSH key
cargo run -p xtask -- remote deploy --host root@ADDRESS    # set up, attest, start: 10 to 20 min the first time
cargo run -p xtask -- demo --remote                        # the console, against the instance
cargo run -p xtask -- remote down                          # stops vLLM and the services; then delete the instance
```

`remote init` writes the tree `.verda/`. It holds:
- the operator's keys;
- a `release.json` whose catalogue is one vLLM model (`--model qwen3-4b --weights Qwen/Qwen3-4B-Instruct-2507 --context 16384 --max-output 2048`, rates in 1 and out 3);
- `remote.json`, with the ports;
- an SSH key pair in `.verda/ssh/`, used for this instance only.

K, the operator's minisign key and the storefront token never leave the laptop.

`remote deploy` copies four things to `$HOME/ca` on the instance: the workspace source (a tar over SSH, without `target/`, `.dev/` or `.verda/`), `release.json`, the dev attestation key, and `ca.env`. Then it runs the scripts in `deploy/verda/`:

1. **`setup.sh`**, once:
   - Ubuntu packages (tor, a C toolchain, cmake, Python's venv);
   - rustup with Rust 1.94.0, building xtask and the services in release;
   - vLLM from PyPI in a venv (`VLLM_SPEC` overrides the version);
   - the model's weights from Hugging Face.
2. **`attest.sh`** records what the instance reports about its TEE in `tree/attest/summary.txt`:
   - an AMD SEV-SNP report from configfs-tsm, whose report data is the SHA-512 of `release.json`, with the launch measurement and the debug bit;
   - the GPU's confidential computing state from `nvidia-smi conf-compute`;
   - NVIDIA's attestation, if `nvattest` is installed.

   It then sets the GPU ready, which CUDA needs in CC mode. It fails unless the CPU is an SNP guest and the GPU is in CC mode.
3. **`start.sh`** starts vLLM on the Unix socket `tree/run/engine.sock` (`--uds`). Request logging, the access log and prefix caching are off. It waits for `/health`, then runs `xtask host`. That starts fake-stripe, the gate and the storefront on 127.0.0.1, and C tor publishing the gate as an onion service. The gate stays Locked.

Deploy then copies back the onion address and the attestation summary.

`remote unlock` and `demo --remote` open an SSH tunnel. It forwards the storefront (8443) and fake-stripe (8090) to the same ports here, and the instance's `gate.sock` to `.verda/run/gate.sock`. Through the tunnel, xtask attests the gate, unlocks it with K, signs the manifest with the gate's issuer key, and gives the storefront its key. Nothing on the instance listens on a public port; SSH is its only inbound. The tunnel carries the purchase plane and the operator's calls only. Chat goes from walletd here through Arti to the instance's onion service.

`demo --remote` runs only walletd, on a fresh wallet in a temp dir, and wallet-cli here. It follows the instance's logs over SSH (`tail -F`) into the activity log. The console must use the port `remote init --console-port` chose (default 7700), because the instance's checkout sends a buyer who paid back there. `remote tunnel` holds the tunnel open alone, `remote status` shows what runs on the instance, and `remote down` stops it.

`remote deploy --local /tmp/DIR` rehearses all this in a local directory. It runs the same scripts, but with fake-engine (`ENGINE=fake`) and `REQUIRE_TEE=0`, and skips `setup.sh`. The directory must have a short path, because the sockets under it must fit in a Unix socket address.

**What the wallet verifies, and what it doesn't yet.** The attestation summary is what the instance says about itself; the console labels it that way. The wallet still checks the gate's dev-insecure attestation only. The SNP report and NVIDIA's evidence are not yet bound to the gate's TLS key, nor verified in walletd.

### A full session by hand

Run these from `prototype/`. The `wallet-cli` default wallet directory is `.dev/wallet`. Set `COUNTERACTIVE_WALLET_DIR` or pass `--wallet-dir` to use another one.

```sh
cargo run -p xtask -- keygen
cargo run -p xtask -- release
cargo run -p xtask -- up            # prints the storefront and fake-stripe addresses
cargo run -p xtask -- unlock        # gate ready, manifest version 1; storefront ready

alias wallet='target/debug/wallet-cli'   # built by `up`
wallet init
wallet manifest fetch --approve      # or `manifest fetch`, then `manifest approve`
wallet buy --product usd-5           # prints order_id and checkout_url, and opens the browser
```

`buy` opens the checkout URL, `http://127.0.0.1:<fake-stripe port>/pay/<order id>`, in your browser. Add `--no-browser` to only print it. The page is fake-stripe's stand-in for Stripe Checkout, showing the product, the price and a **Pay** button. Pressing Pay marks the order paid and redirects to `http://127.0.0.1:7777/paid`, which walletd serves once it runs. Without a browser, `curl -X POST http://127.0.0.1:<fake-stripe port>/test/pay/<order id>` pays the order the same way.

```sh
wallet collect                       # exit 2 while unpaid; afterwards: chain 1 holds 5000000 credits (Ready)
cargo run -p walletd -- --wallet-dir .dev/wallet   # leave it running; listens on 127.0.0.1:7777
```

The wallet page is at **http://127.0.0.1:7777/**. It offers a model selector, streaming chat, the balance and reconcile. walletd embeds its bearer token in the page it serves. It checks the `Host` header and sends a strict Content-Security-Policy. In another terminal:

```sh
wallet models
wallet chat --model fake-small --conversation demo "hello"    # text on stdout, the receipt on stderr
wallet balance
wallet close                          # settles every open session
wallet reconcile
kill -TERM <walletd pid>              # or Ctrl-C; it closes live sessions first
cargo run -p xtask -- down
```

`walletd --listen` takes another loopback address. Keep 7777 if you pay in a browser, because the storefront's Stripe success URL points there. walletd closes a session after 5 minutes of idleness. A purchase is one credit chain, and a chain carries one session at a time. A second conversation started while another is open gets `402 no_spendable_chain` until the first closes, or until you buy again.

### A real Stripe sandbox

```sh
cargo run -p xtask -- down
cargo run -p xtask -- up --real-stripe      # no fake-stripe; the storefront calls https://api.stripe.com
read -rs STRIPE_SECRET_KEY && export STRIPE_SECRET_KEY    # paste an sk_test_ or rk_test_ key
cargo run -p xtask -- unlock
unset STRIPE_SECRET_KEY
```

Only `xtask unlock` reads `STRIPE_SECRET_KEY`. It refuses a key that isn't test-mode (`sk_test_` or `rk_test_`). It sends the key once, over the storefront's attested TLS connection, and holds it in a `SecretString`. The key is never written to disk, a log, `up.json` or a command line. The storefront keeps it in memory only, so after a storefront restart you run `unlock` again with the variable set. `unlock` refuses the variable when the stack runs against fake-stripe, so a real key can't be handed to the fake by mistake. With a real key, `wallet buy` opens Stripe's hosted Checkout. Pay with a Stripe test card such as `4242 4242 4242 4242`, and Stripe redirects to walletd's `/paid`.

### Tier 2: the gate as an onion service

Install C tor so that `command -v tor` finds it, then run:

```sh
cargo run -p xtask -- e2e --tor
```

Step 12 then does the following:
1. Builds walletd with `--features walletd/arti`.
2. Brings up a second stack in its own temp dir. C tor runs from a generated `tor/torrc`, with these lines:
   - `HiddenServicePort 443 unix:<gate.sock>`, which puts the gate behind an onion service;
   - `SocksPort unix:<tor/socks.sock>`, for xtask's own probe and for step 7's direct requests to the gate.
3. Writes the onion address into the manifest (`service.onion`) and into the wallet pins (`gate_target = "onion:<address>.onion:443"`).
4. Waits up to 5 minutes for the gate to answer `ready` through C tor.
5. Runs steps 3 to 10 against that stack, with the same step functions as tier 1. walletd bootstraps Arti in `wallet/arti` and reaches the gate only through the onion service. The operator side (unlock, also after step 8's restart) still uses the gate's local socket. Three things differ from tier 1, because every call to the gate is a round trip through relays:
   - step 8 waits 10 s after killing the gate, so that tor's end-of-stream reaches walletd before the next turn (a turn sent on the dead stream before then is correctly not retried);
   - step 9's `release-idle.json` sets `idle_seconds = 10`, not 2;
   - step 9 gives the abandoned turn 120 s to settle, not 20 s.

This uses the live Tor network. Expect a few minutes for C tor to publish the descriptor and for Arti to bootstrap. Without `--tor`, step 12 is skipped and says why.

If dialling the gate, the TLS handshake or the evidence fetch fails in transit, walletd redials once on a fresh circuit. Nothing has been sent on the failed connection except the public evidence request, so nothing can go out twice. Once a request has been sent, a failure is not retried: a lost turn is counted as unknown and settled by reconcile. Tier 2 can't survive the laptop losing its network, though. On 2026-11-07, four `--tor` runs failed at the moment the Wi-Fi dropped (macOS logged each drop to the minute). Three of those failed while connecting: one before the redial existed, and two where the network was still down when the redial ran. The fourth failed on a stream that was already carrying a request.

## End-to-end results

`cargo run -p xtask -- e2e --tor` was run on 2026-11-07 on macOS (Darwin 25.4, Rust 1.94, tor 0.4.9.16), against the live Tor network. Exit status 0. After the run-directory fix under [Review fixes](#review-fixes), `cargo run -p xtask -- e2e` (steps 1 to 11) was run again and passed.

Step 13, the [kill tests](#kill-tests), came later. `cargo run -p xtask -- e2e` was then run three times on 2026-11-07, without `--tor`. Each run exited 0: steps 1 to 11 and 13 passed, and step 12 was skipped. With warm builds, a whole run took 22 to 29 s.

After tier 2 was extended to steps 3 to 10 and walletd got its redial, `cargo run -p xtask -- e2e --tor` was run twice more on 2026-11-07, at 13:03 and 13:10 local time, with no Wi-Fi drop in either. Both exited 0 with all 13 steps passing, in 4 min 51 s and 8 min 9 s. Over Tor, the steps took:

| Step over Tor | Run 1 | Run 2 |
|---|---|---|
| 5. Chat and metering | 47.5 s | 111.3 s |
| 7. Replay | 16.1 s | 21.0 s |
| 8. Gate crash | 45.0 s | 24.5 s |
| 9. Lost close response and idle close | 32.3 s | 134.4 s |
| 10. Budget errors | 43.4 s | 67.1 s |

Steps 3, 4 and 6 took under a second each.

| Step | Result |
|---|---|
| 1. Bring-up | PASS |
| 2. Attestation negatives | PASS |
| 3. Purchase | PASS |
| 4. Issuance idempotency | PASS |
| 5. Chat and metering | PASS |
| 6. Close and reconcile | PASS |
| 7. Replay | PASS |
| 8. Gate crash | PASS |
| 9. Lost close response and idle close | PASS, including the check of a client that abandons a turn |
| 10. Budget errors | PASS |
| 11. Two-stack check | PASS |
| 12. Tier 2 | PASS: C tor published the gate's onion service, and walletd reached it only through Arti. The first run covered steps 3 to 6; the two later runs covered steps 3 to 10 |
| 13. Kill tests | PASS in every run since it was added, with and without `--tor` |

<details>
<summary>Full output (dev-insecure connection warnings and cargo build lines removed)</summary>

```text
step 1: Bring-up
    ok  keygen and release in /tmp/ca-e2e-ZW5AHg: release.json and release-idle.json
    ok  fake-engine, fake-stripe (127.0.0.1:61768), gate and storefront (127.0.0.1:61769) started
    ok  bootstrap: attested the gate, unlocked it with K, signed its new issuer key into manifest v1 listing 2 releases, posted it
    ok  gate health: ready
    ok  storefront health: ready
    ok  wallet.toml written with the pins
step 1: PASS

step 2: Attestation negatives
    ok  the storefront's evidence checked as the gate: rejected (expected role Gate, evidence claims Storefront)
    ok  gate evidence for one nonce checked against another: rejected (report does not match the expected binding)
    ok  the gate against a manifest without its release: rejected (no manifest release matches the evidence)
    ok  the wallet refuses the manifest signed by another key (manifest: manifest signature does not verify)
    ok  control: the same manifest signed by the pinned operator key is offered for approval
step 2: PASS

step 3: Purchase
    ok  wallet-cli init: journal and key created
    ok  wallet-cli manifest fetch --approve: manifest 1 accepted
    ok  wallet-cli buy --product usd-5 --no-browser: order cs_test_47614b90ec70b2a1276fbf6507791db2, checkout http://127.0.0.1:61768/pay/cs_test_47614b90ec70b2a1276fbf6507791db2
    ok  wallet-cli collect before paying: 402 unpaid, exit 2, order still awaiting payment
    ok  paid through fake-stripe's /test/pay
    ok  wallet-cli collect: chain 1 is Ready with 5,000,000 credits
step 3: PASS

step 4: Issuance idempotency
    ok  re-collecting order cs_test_09d9adb27f7b442fd7dc6c76d44b9d3c with the same request returned identical bytes
    ok  a different issue request for the same order: 409 order_issued
    ok  the right request with a wrong order secret: 401
    ok  the response finalises to a credential of 5,000,000 credits (kept for step 7)
step 4: PASS

step 5: Chat and metering
    ok  walletd up on 127.0.0.1:61781, reaching the gate at unix:/tmp/ca-e2e-ZW5AHg/run/gate.sock
    ok  fake-small turn 1: 14 prompt + 26 completion tokens streamed; charge 14x1 + 26x3 = 92; remaining 499908
    ok  fake-small turn 2: 8 prompt + 20 completion tokens streamed; charge 8x1 + 20x3 = 68; remaining 499840
    ok  a second conversation while the only chain is in a session: no_spendable_chain
    ok  closed conversation `small`
    ok  fake-large turn 1: 9 prompt + 21 completion tokens streamed; charge 9x4 + 21x12 = 288; remaining 499712
    ok  wallet-cli models: fake-large, fake-small
step 5: PASS

step 6: Close and reconcile
    ok  wallet-cli close: the open session closed and its refund finalised
    ok  balance 4999552 = 5,000,000 - 448 charged, exactly
    ok  wallet-cli reconcile: both settled sessions are exact (receipts = s - t)
step 6: PASS

step 7: Replay
    ok  a settled session's spend bytes sent to open again: 409 use_recover
    ok  recover returned the byte-identical refund, which finalises to the full 5,000,000
    ok  journal copied while chain 1 was Ready (walletd stopped for a consistent copy)
    ok  fake-small turn 1: 9 prompt + 21 completion tokens streamed; charge 9x1 + 21x3 = 72; remaining 499928
    ok  a second proof from the same credential, made from the copied journal: 409 spent
    ok  the live wallet is unaffected: its session closed and the balance is exact
step 7: PASS

step 8: Gate crash
    ok  fake-small turn 1: 10 prompt + 22 completion tokens streamed; charge 10x1 + 22x3 = 76; remaining 499924
    ok  gate killed with SIGKILL while the session was open
    ok  gate restarted and unlocked: Ready again from its store
    ok  the next turn saw 404, recovered the spend, and walletd reported it settled as full_refund
    ok  fake-small turn 1: 10 prompt + 22 completion tokens streamed; charge 10x1 + 22x3 = 76; remaining 499924
    ok  balance 4999404 = 4999480 before the crashed session - 76 for the new turn only
    ok  reconcile: the crashed session settled with t = s = 500000 (full_refund), the next one exact
step 8: PASS

step 9: Lost close response and idle close
    ok  gate restarted on release-idle.json (idle_seconds = 2)
    ok  fake-small turn 1: 9 prompt + 21 completion tokens streamed; charge 9x1 + 21x3 = 72; remaining 499928
    ok  after the idle timer, wallet-cli close: the gate answered 404 and walletd settled it through recover
    ok  reconcile: exact, t = 499928 = s - the one receipt
    ok  gate restarted on release.json
    ok  a one-off, non-streaming chat whose engine stalls 3 s: the client hung up after 1 s
    ok  walletd finished the turn and closed its session anyway: exact, one receipt of 76 = s - t
step 9: PASS

step 10: Budget errors
    ok  a 130,000-word prompt on fake-large: 402 prompt_exceeds_budget, required 520,028 > every tier
    ok  the session walletd opened for it settled with t = s and no receipts: balance unchanged
    ok  fake-large turn 1: 100004 prompt + 22 completion tokens streamed; charge 100004x4 + 22x12 = 400280; remaining 99720
    ok  the same prompt with 99720 left: 402 session_exhausted, rolled over once
    ok  fake-large turn 1: 100004 prompt + 22 completion tokens streamed; charge 100004x4 + 22x12 = 400280; remaining 99720
    ok  balance 4198696 = 5,000,000 - 801304 charged over the whole run, exactly
    ok  reconcile: all 10 settled sessions exact, except step 8's crashed one (full_refund)
step 10: PASS

step 11: Two-stack check
    ok  cargo tree -p wallet-cli: no arti-client
    ok  cargo tree -p walletd -e features: no reqwest, no gate-conn "tcp" (control: wallet-cli shows it)
    ok  cargo tree -p gate: no reqwest, no arti-client
step 11: PASS

step 12: Tier 2
    ok  keygen and release in /tmp/ca-e2e-wDww8w: release.json and release-idle.json
    ok  C tor started; the gate's onion service is nxhqawduajbtlkxoxqjasgomtnienadmhczippkm4h7panhv7tgjk4id.onion
    ok  fake-engine, fake-stripe (127.0.0.1:61831), gate and storefront (127.0.0.1:61832) started
    ok  bootstrap: attested the gate, unlocked it with K, signed its new issuer key into manifest v1 listing 2 releases, posted it
    ok  gate health: ready
    ok  storefront health: ready
    ok  wallet.toml written with the pins
    ok  the gate answers `ready` at onion:nxhqawduajbtlkxoxqjasgomtnienadmhczippkm4h7panhv7tgjk4id.onion:443 through C tor
    ok  wallet-cli init: journal and key created
    ok  wallet-cli manifest fetch --approve: manifest 1 accepted
    ok  wallet-cli buy --product usd-5 --no-browser: order cs_test_1a4d0224f525a211185c4b3986388fe0, checkout http://127.0.0.1:61831/pay/cs_test_1a4d0224f525a211185c4b3986388fe0
    ok  wallet-cli collect before paying: 402 unpaid, exit 2, order still awaiting payment
    ok  paid through fake-stripe's /test/pay
    ok  wallet-cli collect: chain 1 is Ready with 5,000,000 credits
    ok  re-collecting order cs_test_16ac60efb6f1808eb20b30d3a85d1ddd with the same request returned identical bytes
    ok  a different issue request for the same order: 409 order_issued
    ok  the right request with a wrong order secret: 401
    ok  the response finalises to a credential of 5,000,000 credits (kept for step 7)
    ok  walletd up on 127.0.0.1:61859, reaching the gate at onion:nxhqawduajbtlkxoxqjasgomtnienadmhczippkm4h7panhv7tgjk4id.onion:443
    ok  fake-small turn 1: 14 prompt + 26 completion tokens streamed; charge 14x1 + 26x3 = 92; remaining 499908
    ok  fake-small turn 2: 8 prompt + 20 completion tokens streamed; charge 8x1 + 20x3 = 68; remaining 499840
    ok  a second conversation while the only chain is in a session: no_spendable_chain
    ok  closed conversation `small`
    ok  fake-large turn 1: 9 prompt + 21 completion tokens streamed; charge 9x4 + 21x12 = 288; remaining 499712
    ok  wallet-cli models: fake-large, fake-small
    ok  wallet-cli close: the open session closed and its refund finalised
    ok  balance 4999552 = 5,000,000 - 448 charged, exactly
    ok  wallet-cli reconcile: both settled sessions are exact (receipts = s - t)
step 12: PASS
```

</details>

Every receipt is checked against the fake engine's own formulas and the release's rates:
- prompt tokens are 4 plus the prompt's words;
- completion tokens are `16 + words % 17`, and the streamed text must have exactly that many words;
- the charge is prompt tokens times the input rate plus completion tokens times the output rate, and `remaining` is carried from one receipt to the next.

The expected balance is 5,000,000 minus the sum of every counted charge. It is compared exactly after steps 6, 7, 8, 9 and 10.

Step 9 ends with a check SPEC doesn't list. A client sends a one-off, non-streaming chat to walletd's API while the engine stalls for 3 s, and hangs up after 1 s. walletd must finish the turn, journal its receipt and close the session, so the chain is `Ready` again and the session settles `exact`. Before the fix described under [Review fixes](#review-fixes), the same check failed: 20 s after the client hung up, the chain was still `Open`.

### Kill tests

Step 13 builds `gate`, `wallet-cli` and `walletd` again with their `crash-points` feature, into `crash-points/` inside the target directory. So the binaries steps 1 to 12 run are never replaced. The step first checks that those binaries carry no crash point, with the crash-point builds as the control:

- `cargo tree -e features -i crash-point` shows no `crash-point feature "enabled"` for the packages built together, or for walletd built alone;
- the binaries don't contain the string `COUNTERACTIVE_CRASH_POINT`.

It then brings up a stack of its own from the crash-point builds. Each test arms one point through `COUNTERACTIVE_CRASH_POINT`, gets a fresh wallet with one new order, and requires the armed process to die by SIGABRT at that point. The process's log must name the point. The test then restarts the process unarmed on the same data and checks the result.

| Point | Where | After the restart |
|---|---|---|
| `after_issue_commit` | gate, issuance recorded, answer not sent | `collect` waits on a 503. The journalled request re-collects identical bytes, and another request gets `409 order_issued`. The retry gives one chain |
| `after_insert_pending` | gate, Pending row durable, no slot | The chain is `Spending`. Reconcile settles `t = s` with no receipts. Recover returns the same refund twice |
| `before_settle` | gate, refund sealed, not committed | The chain is `Closing` with the turn's receipt. Reconcile settles `t = s`, `full_refund`, so the turn costs nothing |
| `after_settle` | gate, refund committed, close not answered | Recover returns `t = s − charge`, byte-identical before and after the wallet settles. Reconcile reports `exact` |
| `after_journal_purchase` | wallet-cli `buy`, before `POST /v1/orders` | The purchase is `Ordered` with no order id. `collect` sends the order from the same secret and issue request |
| `after_create_order` | wallet-cli `buy`, order created, id not journalled | The same, and the order the crash left was never shown, so it can't be paid |
| `after_finalize_issue` | wallet-core, credential finalised, not journalled | The purchase is still `AwaitingPayment` with the same request, and there is no chain. The retry gives one chain |
| `after_journal_spend` | wallet-core, spend journalled, not sent | The gate holds no session. walletd's startup reconcile resends the bytes and settles `t = s`, `exact` |
| `after_open_response` | walletd, gate's `open` answer not journalled | The gate holds one session. The resent bytes get that session back, and it settles `t = s`, `exact` |
| `before_finalize_refund` | wallet-core, refund received, not finalised | Recover returns the committed `t = s − charge`. walletd's startup reconcile settles it `exact` |

Every test ends with the balance walletd reports, which must equal 5,000,000 minus the charges on the receipts of committed sessions. That total must sit in one `Ready` chain, with nothing frozen. Where a spend was in flight, recover with the bytes journalled before the crash must return the refund the wallet settled. The gate seals a refund under the spend bytes, so this shows the retry resent those bytes rather than a new proof.

The wallet journals a collected credential with its purchase, and a settled credential with its session, in one transaction each. So no point lies between finalising and deleting state.

As a negative control, `after_finalize_issue` was moved after the journal commit, and step 13 failed as it should: after the crash, the purchase was already `Collected`. The point was then put back.

On macOS each abort leaves a crash report in `~/Library/Logs/DiagnosticReports`, so every run adds ten.

### Unit and integration tests

`cargo test --workspace`: 226 passed, 0 failed, 0 ignored, across 26 test binaries. `cargo test -p crash-point --features enabled`: 4 passed.

| Crate | Tests |
|---|---|
| `proto` | 43 |
| `gate` | 15 unit, plus 12 in `tests/tier1` |
| `attest`, `credit`, `manifest`, `wallet-core` | 16, 14, 14, 15 |
| `storefront` | 13 unit, plus 6 in `tests/storefront` and 4 in `tests/fake_stripe` |
| `metering` | 12 |
| `backend` | 7 unit, plus 6 in `tests/fake_engine` |
| `fake-engine`, `fake-stripe` | 4, 7 |
| `gate-conn` | 2 unit, plus 5 in `tests/attested_connection` |
| `crash-point`, `xtask` | 2, 2 |
| `walletd` | 6 unit, plus 12 in `src/tests` that run walletd in process against a real gate and fake engine on Unix sockets: the local API's Host and token checks, chat with rollover, recover and `use_recover`, close and reconcile; the idle close; the redial after a stalled connection |
| `wallet-cli` | 9 in `tests/cli`, which run the built binary: `buy` journalling before any network call, `collect`'s exit statuses against a storefront on fake-stripe, argument handling, and the `--json` output xtask decodes, against a stub walletd |

The release build, `cargo check -p walletd --features arti`, `cargo clippy --workspace --all-targets` and `cargo fmt --check` are all clean.

### Mutation checks

To check that the new walletd tests catch real bugs, each mutation below was applied, by itself, to a copy of the tree, and walletd's tests were run against it. A mutation counts as caught when at least one test fails.

| Mutation | Result |
|---|---|
| No redial after a connection fails in transit | Caught by all three redial tests |
| A second redial (three dials to an unreachable gate) | Caught: `an_unreachable_gate_is_dialled_twice_and_nothing_is_spent` |
| The idle close never closes | Caught by both idle tests |
| The idle close ignores the idle time | Caught (after the fix below) |
| A turn's end doesn't count as use, so a session is closed straight after a long turn | Caught (after the fix below) |
| No `Host` check on the local API | Caught: `the_host_header_must_name_this_walletd` |
| No rollover on `402 session_exhausted` | Caught: `an_exhausted_session_rolls_over_once` |

The two idle-time mutations first survived. Neither idle test checked that a session stays open until the idle time has passed, so a walletd that closed a session the moment its turn ended passed both. `a_conversation_in_the_middle_of_a_turn_is_not_closed` now runs with a 1 s idle time and checks, half an idle time after the turn ends, that the session is still open. It passed three runs in a row on the real code, and it fails both mutations. The kill tests' own negative control is under [Kill tests](#kill-tests): it moved `after_finalize_issue` after the journal commit, and step 13 failed.

### Browser check

On 2026-11-07 a manual session was driven in Chrome against the `.dev` stack with walletd on `127.0.0.1:7777`. It was not automated, so it is not part of `e2e`.

| Check | Result |
|---|---|
| fake-stripe checkout page, **Pay** button | Paid and redirected to walletd's `/paid`; `wallet-cli collect` then gave chain 1 with 5,000,000 credits |
| Wallet page: model selector, chat, balance, Refresh, Reconcile | All work. Reconcile shows the same per-session lines as `wallet-cli reconcile` |
| Streaming | A 4,096-token `[[long]]` reply was appended in 1,816 DOM updates. walletd forwards frames through a channel into a streaming body, and the page reads them with `getReader()` |
| Charges | Each matched the fake engine's formulas, for example 12 × 1 + 24 × 3 = 84. Four sessions settled `exact`, and the balance ended at 5,000,000 − 29,621 = 4,970,379 |
| HTML in a prompt | `<img src=x onerror=...><b>bold</b>` showed as text: no element was created and the handler never ran. `app.js` writes only with `textContent` |
| Headers | CSP `default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'`, and `nosniff`. No console errors |
| Local API | No or wrong token: 401. `Host: evil.example`: 403, for the API and the page. A cross-origin preflight gets 401 with no CORS headers. The token appears in no log |

It found one bug, now fixed in `crates/walletd/page/app.js`. **New conversation** closed the old session before switching to a new conversation, so a message sent during the close went to the old conversation with its whole history. In the test it was charged 16,876 credits instead of 256. It also reopened a session there, which held the only chain, so the new conversation got `no_spendable_chain` until walletd's 5-minute idle close. The page now switches conversation first, then closes the old one, and disables Send and New conversation while a turn or a close is in flight. A turn refused before any text arrives is also removed from the page, and its prompt is put back. Rerunning the same sequence showed the stray Send ignored and the new turn charged 256.

### Demo runs

On 2026-11-07, `xtask demo` was run on the same laptop against the live Tor network, both headless and driven in Chrome through the DevTools protocol.

| Run | Result |
|---|---|
| `demo --check --local`, three times | PASS in 2.0 s each. Turn 1 charged 9 + 21 × 3 = 72, turn 2 charged 46 + 16 × 3 = 94, s 500,000 − t 499,834 = 166, exact; balance 999,834 |
| `demo --check` over Tor | PASS in 17.4 s. walletd's use-plane fetch through Arti took 8.5 s, turn 1's first text 6.1 s, turn 2's 0.9 s. Same charges and settlement as above |
| `demo --check` over Tor, on the final build | PASS in 18.8 s. The use-plane fetch took 5.7 s, turn 1's first text 8.8 s, turn 2's 1.3 s. Same charges and settlement |
| `demo --check` over Tor, with a second Tor demo running on the same machine, before the use-plane check existed | PASS in 496.5 s, of which turn 1 waited 493.5 s for its first text |
| Chrome, first run | Ready in 19.2 s. Manifest approved, `usd-1` bought, paid on fake-stripe's page, collected as chain 1. The first session's first token came after 47.0 s, later turns after about 0.9 s, and a second conversation's first token after 11.4 s. Three turns on both models charged 88 + 130 + 608 = 826 = s − t, exact |
| Chrome, after the fixes below | Ready in 19.6 s. The use-plane check took 8.0 s. The first session's first token came after 18.8 s, turn 2 after 0.9 s, and a second conversation's first token after 5.4 s. Two turns charged 84 + 456 = 540 = s − t, exact; balance 999,460. No console messages |
| Chrome, third run | Ready in 212.8 s: C tor bootstrapped in 12 s, but the onion service first answered xtask's probes about 200 s later. The use-plane check took 6.2 s. The first session's first token came after 9.4 s, turn 2 after 1.3 s. Two turns charged 72 + 129 = 201 = s − t, exact; balance 999,799. No console messages. Ctrl-C stopped every service and removed the temp dir |
| `demo --check --local`, after the check's prompts became counting prompts for a real model | PASS in 2.5 s. Turn 1 charged 14 + 26 × 3 = 92, turn 2 charged 54 + 24 × 3 = 126, s − t = 218, exact; balance 999,782 |
| `remote deploy --local /tmp/ca-reh`, then `demo --remote --check` | PASS in 15.6 s. The rehearsal ran `attest.sh` and `start.sh` (fake-engine, fake-stripe, the gate, the storefront and C tor, through `xtask host`). The demo unlocked the gate through the rehearsal's tunnel, which there is a symlink to its `gate.sock`. The use-plane fetch through Arti to the rehearsal's onion took 6.2 s, turn 1's first text 5.7 s, turn 2's 0.8 s. Same charges as the line above. In Chrome, the page named the rehearsal and fake-engine in the bring-up text, the engine hop and the transcript, and listed the instance's components |

The first Chrome run found six problems, all fixed:

- **The collector flooded the log.** wallet-cli's dev-insecure warning carries a timestamp, so every 2 s `collect` looked new and was logged again. Timestamps are now stripped before comparing, and each dev-insecure warning is shown once per source.
- **The page polled the balance before a manifest was accepted.** walletd answers `409` then, and the browser logged each one as an error. The page now polls only after acceptance.
- **The browser asked for `/favicon.ico` and got 404.** It now gets `204`.
- **The component list's plane tag used the class `purchase`**, which the purchase list styles as a bordered card. The tags are now `plane-*`.
- **The manifest facts listed `0.1.0-dev, 0.1.0-dev`**, one per release. They now show the release count and the distinct images and platforms.
- **The balance showed `$1.000000`.** It now shows `$1.00`, keeping digits only where they mean something: `$0.99946`.

The 47 s first session also led to the use-plane check after approval. Besides checking that the onion serves the approved manifest, it builds Arti's first path to the onion service while the user is still buying. The chat card also says why a new session's first message waits, and counts the seconds.

In the third Chrome run, the log showed nothing between "waiting for C tor to publish the onion service" and the gate's answer 210 s later. Now, every 30 s of that wait, the log shows the last probe's error. C tor's own notice log showed no warning for the delay, so its cause is unknown.

## Review fixes

Two reviews of the prototype reported nine findings. Seven were fixed, each with a regression test:

- **A client that hung up stranded its chain** (medium). walletd ran each money flow inside the request's handler, so a client that left dropped the flow mid-way. The chain stayed in a session that no conversation held, still marked as in use, so reconcile skipped it and every new chat got `no_spendable_chain` until walletd restarted. A dropped non-streaming turn also lost its receipt, so the session later settled as `mismatch` instead of `receipt_gap` (low). Now chat, close and reconcile run in their own task to the end, as the gate's flows do. A chain is marked as in use by a guard that releases it when dropped, however the flow ended. Tests: step 9's abandoned-turn check, and unit tests for the task and the guard.
- **A flow holding an old spend could act on the chain's next one** (medium). Two flows could drive one orphaned chain at once, for example two reconciles. The slower one could then resend a settled spend while a chat had started the next spend, and either freeze the chain or close the chat's new session. Now reconcile marks its orphans as in use while it holds the journal lock, and a chat marks its chain in the same critical section that journals the spend (this also fixes a narrower race, low). Every journal transition within a spend names its `spend_seq` and is refused for any other spend, and so is settling a refund. Test: `a_stale_spend_cannot_touch_the_next_one` in `wallet-core`.
- **A second walletd on the same wallet closed the first one's sessions** (low). walletd now holds an exclusive lock on `walletd.lock` and refuses to start while another walletd holds it. Test: `lock::tests`, plus a manual run of the binary against a held lock.
- **walletd accepted any existing `walletd.token`** (low). It now refuses a token file that other users can read, or one that doesn't hold a 64-hex-digit token, as it already did for `journal.key`. Test: `token::tests`.
- **The gate trusted an existing run directory** (low). It binds `gate.sock` and `issue.sock` before setting their mode to 0660, so for a moment only the directory keeps other users out. `issue.sock` mints credit without further authentication. The gate now refuses an existing run directory that its group can write or other users can enter. Test: `serve::tests::the_run_dir_must_keep_others_out`.

Two were confirmed and deferred, because neither fix is small. They are listed in the next section.

## What is not implemented or not verified

- **Tier 2 has passed twice, from one laptop.** Steps 3 to 10 run over Tor, but steps 1, 2 and 11, the operator's unlock and the kill tests use the gate's local socket. Both Tor ends ran on one machine, with circuits through real relays. Step 8 over Tor waits 10 s after killing the gate, so that tor's end-of-stream reaches walletd before the next turn. A turn sent before then fails once with `502` and is not resent, which is correct but untested. Tier 2 fails whenever the laptop's network drops.
- **Real Stripe has never been called.** `up --real-stripe` and the `STRIPE_SECRET_KEY` path are implemented. The storefront's Stripe client is exercised only against fake-stripe.
- **The wallet page has no automated test.** It was checked by hand in a browser (see [Browser check](#browser-check)). No test drives `app.js`, so a regression there would show up only in another manual check.
- **The demo console's page script is not tested automatically either.** `demo --check` drives the console's HTTP API with the same calls `app.js` makes and checks every number, but no browser runs `app.js` in a test. The page was driven by hand in Chrome (see [Demo runs](#demo-runs)).
- **walletd's own tests use only the Unix dialer, and wallet-cli's use-plane tests use a stub walletd.** walletd's idle close is tested with 300 ms and 1 s timers, which `close_idle` takes as an argument; the binary still runs it at 5 minutes. The Arti dialer runs only in tier 2, and the 5-minute timer itself in no test. wallet-cli's use-plane commands meet a real walletd only in e2e.
- **Kill tests cover only the named crash points, one at a time, over the Unix socket.** Step 13 arms each of the ten points once. It never arms two points in one run, and it doesn't crash the storefront, which has no crash points. Crash points don't exercise races; the gate's slot interleavings have their own tier 1 tests. No kill test runs through Tor.
- **Approving a manifest isn't bound to the one the user reviewed.** `manifest approve` (and `POST /wallet/manifest {action: "approve"}`) accepts whatever is pending, so a second fetch between review and approval swaps what gets approved. A manifest with the same version but different content is also accepted. That also lets the verdict cache, which is keyed by manifest version, keep a verdict for up to an hour for a release the new manifest dropped. Exploiting this needs the operator's signing key.
- **No lost close response is injected in transit.** The case it stands for, a close the gate settled whose answer the wallet never saw, is covered by step 7: recover after a settled close returns the byte-identical refund. Step 13 also loses that answer by killing a process: the gate at `after_settle`, and walletd at `before_finalize_refund`. Nothing drops it while both stay up. Step 9 checks the idle close that SPEC describes.
- **One chain carries one session.** Concurrent conversations need more than one purchase.
- **Left out by SPEC** (see the top of this file): drain, export, import, SNP, GPU appraisal, the boot collector, the image, release CI, proof-of-work and the OS keychain. The journal key sits in `journal.key` (mode 0600) in place of the keychain.

## Deviations from SPEC

- **Step 5 order.** One purchase is one chain, and a chain carries one session (docs 04 section 7). So the `fake-large` conversation can't run while the `fake-small` one is open. Step 5 checks that it gets `402 no_spendable_chain`, closes the first conversation, then runs the `fake-large` turn. Step 6 then closes the one open session.
- **wallet-cli additions.**
  - A global `--json` flag prints walletd's answers as JSON; e2e reads it.
  - `--wallet-dir` (or `COUNTERACTIVE_WALLET_DIR`) sets the wallet directory.
  - `manifest approve` accepts a fetched manifest without `--approve`.
  - `chat` reads the prompt from stdin when it is `-`. e2e sends 130,000-word prompts that way.
  - `collect` exits 2 while orders are unpaid.
- **walletd flags and files.** `--listen` (default `127.0.0.1:7777`, loopback only) and `--wallet-dir`. walletd writes `walletd.addr` so wallet-cli finds it. e2e uses port 0, so tests never bind a fixed port. walletd also holds an exclusive lock on `walletd.lock` in the wallet directory, so only one walletd serves a wallet.
- **Step 9 checks an abandoned turn as well.** See [End-to-end results](#end-to-end-results).
- **xtask additions.**
  - `up --real-stripe` runs against a Stripe sandbox.
  - `--dev-dir` picks the dev tree.
  - The tree also holds `logs/` and `up.json`. e2e adds `release-idle.json` (step 9) and `tor/` (tier 2).
  - The manifest lists both releases.
  - `demo` serves the browser console described under [The browser demo](#the-browser-demo).
- **fake-engine `--token-delay-ms`.** A pause before each streamed token, default 0. The demo sets 40 ms so replies arrive visibly token by token.
- **Temp dir under `/tmp`.** e2e's temp dir is in `/tmp`, not `$TMPDIR`, because macOS limits Unix socket paths to about 104 bytes.
- **Step 12 runs only with `--tor`.** Without the flag it is skipped even when tor is installed, as SPEC's "with `--tor` only" says.
- **walletd is built in its own cargo invocation.** Cargo unifies features across the packages of one build, which would give walletd's `gate-conn` the TCP dialer that wallet-cli enables. The walletd binary xtask builds has no `TcpDialer` symbol. A plain `cargo build --workspace` still unifies features, but step 11's `cargo tree -p walletd` check is per package either way.
- **Logging.** The gate and fake-engine now log to stderr without colour codes when stderr isn't a terminal, like the other services, so their log files read cleanly.

## Secrets

These secrets never reach logs or `Debug` output: K, the issuer seed, spend bytes, client states, credentials, the Stripe key, the operator and storefront tokens, and the journal key. They are held in `secrecy` or `zeroize` types. Request and response bodies are never logged.

After a manual run, each service's log was searched for K and for the Stripe key; neither appeared. Key files are created with mode 0600 and directories with 0700, and `keygen` refuses to overwrite keys.

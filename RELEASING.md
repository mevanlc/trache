# Releases

This repository releases one Rust package containing the `trache` command.
It publishes nine binary archives and one crate from the
same commit, following the [tuisv release setup](https://github.com/mevanlc/tuisv/blob/main/RELEASING.md).
The Git tag supplies the version; committed package versions stay at `0.0.0`.

## Normal releases

1. Merge changes into `main` and let CI pass.
2. Tag the intended commit and push that tag:

   ```sh
   git tag -a v0.4.0 -m 'Release v0.4.0'
   git push origin v0.4.0
   ```

Use a new version for each publication. Stable tags have the form `vX.Y.Z`.
Prereleases such as `v0.4.0-rc.1` are supported; build-metadata suffixes are not.
Prereleases are published to crates.io and marked as prereleases on GitHub,
without becoming the stable latest release.

The release workflow validates and stamps the version, runs all checks, builds
and packages every target, stages a complete GitHub draft, publishes the crate,
then publishes the draft. Immutable releases lock the tag and assets and include
GitHub's release attestation. Generated source-code archives from GitHub reflect
the committed `0.0.0` manifest; the attached `.crate` contains the stamped version.

## First publication and credentials

Enable immutable releases in the repository's release settings. Create the
`crates-io` GitHub Actions environment without required reviewers, so tag releases
publish automatically.

For a new crate, log into crates.io, verify the account email, and create a
short-lived API token that can create/publish this crate. Add it as the
`CARGO_REGISTRY_TOKEN` secret in the `crates-io` environment using GitHub's secret
UI or `gh secret set CARGO_REGISTRY_TOKEN --env crates-io`. Enter it through the
hidden prompt; never put the value in source, command arguments, logs, or chat.

After the first publication:

1. In the crate's crates.io settings, add a GitHub trusted publisher:
   owner `mevanlc`, repository `trache`, workflow `release.yml`, environment `crates-io`.
2. Run Release manually on `main` with mode `verify-oidc`. It obtains a temporary
   token and automatically revokes it at job completion; it cannot publish.
3. Delete the bootstrap `CARGO_REGISTRY_TOKEN` secret. Revoke its crates.io token
   only if it was dedicated to this bootstrap; keep a shared token valid for
   other projects that still use it.

The workflow uses the bootstrap secret when present and otherwise uses OIDC.
An already-published matching crate needs neither credential mechanism to be
uploaded again. Tokens are supplied only to the publishing step; builds run
without publishing credentials.

## Validation without publishing

Run Release manually with mode `build` and a version such as `0.4.0` or
`0.4.0-rc.1`. This runs the complete build and package checks and stores Actions
artifacts. Manual runs cannot create releases or publish crates.

Pull requests and pushes to `main` run quality checks, all nine platform builds,
and crate packaging/install checks. They use version `0.0.0` and do not publish.

Local checks:

```sh
cargo fmt --all --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo nextest run --locked --all-targets --all-features --test-threads 1
cargo test --doc --locked --all-features -- --test-threads=1
uv run scripts/test_release.py
actionlint
cargo publish --dry-run --locked
```

Use a disposable checkout for stamping tests:
`uv run scripts/release.py stamp 0.4.0` deliberately edits the local manifest and
the package's lockfile entry. It verifies that no dependency versions change.
The helper assumes it is run from the repository root.

The eight desktop/server targets execute tests and smoke-test the extracted
download with `--help` and `--version`. FreeBSD x64 and ARM64 build and test
inside FreeBSD 15.1 VMs; the extracted downloads also exercise dry runs,
trashing, restoration, and purging using a temporary trash directory. FreeBSD
x64 and ARM64 run tests from a temporary source copy in the VM user's home
directory so `gio` can use the same filesystem as its home trash.
FreeBSD
x64 uses nextest; ARM64 uses `cargo test` because nextest does not publish a
FreeBSD ARM64 binary. ARM64 installs the pinned Rust toolchain from its official
standalone archive because rustup does not publish an installer for that target.
FreeBSD downloads are tested on 15.1. Linux binaries are checked for static
linking. Android uses NDK 27.3.13750724 with API 24 and is checked for the correct
ELF architecture, interpreter, and system-library dependencies. Android execution
is not covered by hosted CI. macOS uses deployment target 11.0.

Crate verification compiles the packaged source and installs from its unpacked
contents. Before publishing, the upload job recreates the crate and checks its
SHA-256 against the verified CI artifact. Its `cargo publish --no-verify` skips
only the compilation already performed by CI.

## Downloads and verification

Binary archives use `trache-vVERSION-TARGET.tar.gz`, or `.zip` for Windows, and
contain a matching top-level directory with the executable, README, release guide, MIT
license, `licenses/UNLICENSE`, and the vendored backend notice in
`trash-patched/LICENSE.txt`. Each release also contains:

- `trache-VERSION.crate`: the exact crate published to crates.io.
- `release-manifest.json`: source commit, Rust toolchain, target matrix, and hashes.
- `SHA256SUMS`: hashes for all the above assets, including the manifest.

Download all assets into an empty directory and verify them:

```sh
gh release download v0.4.0 --repo mevanlc/trache
sha256sum --check SHA256SUMS
```

On macOS, use `shasum -a 256 --check SHA256SUMS`. On Windows, use
`Get-FileHash -Algorithm SHA256` to compare a file with its entry in `SHA256SUMS`.
GitHub's `gh release verify` and `gh release verify-asset` additionally verify
the immutable release attestation.

## Recovery

Start by rerunning failed jobs in the original Actions run. Successful build
artifacts remain available for 30 days; the assembled release bundle is retained
for 90 days. Retrying a failed publication reuses that run's successful artifacts.

- Before publication, a failure leaves the draft and any successfully uploaded
  assets in place.
- Existing assets must have matching hashes. Only missing assets are uploaded.
  Differing or unexpected assets stop the workflow; nothing is clobbered.
- If crate upload succeeded but a later operation failed, the retry checks the
  registry checksum and skips the upload only when the bytes match.
- If Cargo times out after uploading, the helper polls the registry for up to
  five minutes before failing; retrying reconciles an upload that appeared later.
- A completed matching release is a successful no-op on retry.

Do not move tags, delete releases, or replace published versions to fix an
application bug. Publish a new version. If a full rebuild differs from an
existing asset, resume with the original run's artifacts; pinned tools and
normalized archive metadata do not make hosted runner images immutable.

## Patched trash backend and tests

The crate includes the patched `trash` 5.2.5 sources under `trash-patched/` as
its `trash` library target. There is no unpublished path dependency and no
separate backend publication. This preserves the local macOS, Windows, and
Android fixes in both binary downloads and crates.io installs. Upstream metadata
is retained in `Cargo.toml.upstream`, `Cargo.toml.orig`, and `.cargo_vcs_info.json`;
the root manifest supplies the actual dependencies and features. Keep the
upstream MIT notice with these sources and in every release archive.

The backend's unit tests run with the CLI tests in the root package. Tests run
serially because Trash and Recycle Bin operations share operating-system state.
Doctests run separately and serially because nextest does not execute them.
macOS tests include Finder and mounted HFS+ volume checks; Windows tests use the
native Recycle Bin. Run the full suite on disposable machines or hosted CI.

## Adapting this example

Copy the two workflows, the release helper and tests, the target matrix,
`rust-toolchain.toml`, `.gitattributes`, and the relevant packaging/license metadata. This is a
copyable example, not a centrally hosted workflow dependency.

- Update the Cargo package/binary names, repository URL, description, license,
  and documentation. Update the executable loops and crate paths in CI;
  the helper derives the
  package and binary names from the manifest's explicit `[[bin]]` entries.
- Adjust `scripts/release-targets.json` and platform setup for the new project.
  Its entries also define the expected complete release asset set.
- Keep test fixtures and other compile-time files in the crate's `include` list.
- Configure immutable releases, the `crates-io` environment, and the new crate's
  trusted-publisher identity. Each new crate needs its own initial token publish.
- Update Rust in `rust-toolchain.toml`. Both workflows read that pin. Update uv,
  Python, nextest, actionlint, and Android NDK pins deliberately and rerun the
  full manual build. Dependabot proposes updates to pinned GitHub Actions monthly.

The helper supports one package with explicitly listed executables. A workspace
needs explicit dependency-order publishing and is outside this example.

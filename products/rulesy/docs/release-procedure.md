# Rulesy release procedure

Rulesy releases are driven by annotated `vX.Y.Z` Git tags. Pushing the tag
starts the repository's [`Release` workflow](../../../.github/workflows/release.yml).
The workflow builds and verifies the supported artifacts, creates the GitHub
release, and publishes the Rulesy development-container Feature. It does not
use Go, GoReleaser, or a Go version file.

## 1. Prepare `main`

Work from a clean `main` branch whose `HEAD` exactly matches `origin/main`.
Run the normal Rulesy test and quality gates before releasing:

```bash
moon run rulesy:format
moon run rulesy:lint
moon run rulesy:test
```

The release helper checks these conditions itself and aborts before changing
the version when they are not satisfied.

## 2. Cut the release

From the repository root, run:

```bash
moon run rulesy:release -- patch
```

Use `minor` or `major` instead of `patch` when appropriate.

The `release` task delegates to
[`products/rulesy/scripts/release.sh`](../scripts/release.sh), which:

1. fetches `origin/main`;
2. requires the current branch to be clean `main` at the fetched commit;
3. reads `products/rulesy/src/version.rs`;
4. increments the selected semantic-version component;
5. commits that file as `Release vX.Y.Z`;
6. creates the annotated `vX.Y.Z` tag;
7. pushes `main`, then the tag, to `origin`.

These Git writes are intentional release actions. Inspect the proposed version
and confirm that CI is healthy before running the helper.

## 3. Automated build and publication

For the pushed tag, the release workflow first verifies that the tag matches
the Rust `VERSION` constant through
`rulesy:ensure-tag-matches-version`. It then invokes
`rulesy:cross-compile` for each target and packages:

| Target | Archive |
| --- | --- |
| `x86_64-unknown-linux-musl` | `rulesy_linux_x86_64.tar.gz` |
| `aarch64-unknown-linux-musl` | `rulesy_linux_aarch64.tar.gz` |
| `x86_64-apple-darwin` | `rulesy_darwin_x86_64.tar.gz` |
| `aarch64-apple-darwin` | `rulesy_darwin_aarch64.tar.gz` |

Each build also emits an archive checksum. The workflow:

1. runs the x86-64 and ARM64 Linux binaries natively to verify the static
   release contract and a real Rulesy provisioning smoke test;
2. combines the per-archive checksums into `checksums.txt`;
3. signs that file as `checksums.txt.sig` with the configured release key;
4. creates or updates the GitHub release and uploads all archives, checksums,
   and the signature; and
5. publishes the unchanged `rulesy` Feature identity from
   `devcontainer-features/src`.

Publishing happens only in the tag workflow. Local build and cross-compile
commands do not publish artifacts.

## 4. Verify the release

After the workflow succeeds:

1. open the repository's GitHub Releases page;
2. confirm all four archives, `checksums.txt`, and `checksums.txt.sig` exist;
3. confirm both native Linux verification jobs passed;
4. confirm the published development-container Feature still uses the
   `rulesy` identity and resolves the new version; and
5. install into a clean environment through the stable public installer:

   ```bash
   curl -fsSL https://raw.githubusercontent.com/notwillk/rulesy/main/scripts/install.sh | bash
   rulesy --version
   ```

The reported version must match the tag. A release is complete only after the
GitHub artifacts and Feature publication have both been verified.

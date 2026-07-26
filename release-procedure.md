# Release Procedure

Releases are fully automated and driven by **git tags**.
Pushing a `vX.Y.Z` tag triggers the `Release` GitHub Actions workflow, which builds and verifies the supported binaries, creates a GitHub Release, uploads all artifacts, and publishes the Rulesy development-container Feature.

---

## 1. Ensure `main` is ready

Make sure `main` has all the commits you want to ship:

```bash
git checkout main
git pull
```

Run tests locally if desired.

---

## 2. Run the release script

Use the helper script to bump the version, commit, tag, and push everything in one go. The script enforces that `main` is clean and up to date with `origin/main` before proceeding.

```bash
moon run rulesy:release -- patch   # or minor / major
```

The script:

1. Reads the existing package version from `products/rulesy/Cargo.toml`
2. Increments it according to the argument
3. Updates `products/rulesy/Cargo.lock` and commits the manifest and lockfile
   with message `Release vX.Y.Z`
4. Creates an annotated tag `vX.Y.Z`
5. Pushes `main` and the new tag to `origin`

Once the tag is pushed, the release workflow starts automatically.

---

## 3. Let GitHub Actions handle the release

Once the tag is pushed:

1. The **Release** workflow runs automatically.
2. The workflow:
   - Builds binaries for all configured OS/architecture combinations
   - Packages them as archives named:

     ```text
     rulesy_<os>_<arch>.tar.gz
     ```

   - Generates and signs the checksum manifest
   - Creates a GitHub Release for `vX.Y.Z` (if one does not already exist)
   - Uploads all artifacts to that Release
   - Publishes the Rulesy development-container Feature

You can watch progress under:

```text
Actions → Release
```

---

## 4. Verify the release (optional)

After the workflow finishes:

1. Open:

   ```text
   https://github.com/notwillk/rulesy/releases
   ```

2. Confirm the `vX.Y.Z` release has:
   - All platform archives
   - `checksums.txt` and `checksums.txt.sig`
   - A successful workflow run
   - The published Rulesy development-container Feature

---

## 5. Test installation (optional)

If using a curl installer script:

```bash
curl -fsSL https://raw.githubusercontent.com/notwillk/rulesy/main/scripts/install.sh | bash
rulesy --version
```

Confirm the installed version matches the release.

---

## Summary

1. Ensure `main` is up to date
2. Run `moon run rulesy:release -- patch|minor|major`
3. GitHub Actions handles everything once the tag is pushed

**Release is complete.**

# Release Procedure

Releases are fully automated and driven by **git tags**.  
Pushing a `vX.Y.Z` tag triggers the `Release` GitHub Actions workflow, which runs GoReleaser, creates a GitHub Release, and uploads all artifacts.

---

## 1. Ensure `main` is ready

Make sure `main` (or your release branch) has all the commits you want to ship:

```bash
git checkout main
git pull
```

Run tests locally if desired.

---

## 2. Run the release script

Use the helper script to bump the version, commit, tag, and push everything in one go. The script enforces that `main` is clean and up to date with `origin/main` before proceeding.

```bash
just release patch   # or minor / major
```

The script:

1. Reads the existing version from `src/internal/version/version.go`
2. Increments it according to the argument
3. Commits the version bump with message `Release vX.Y.Z`
4. Creates an annotated tag `vX.Y.Z`
5. Pushes `main` and the new tag to `origin`

Once the tag is pushed, the release workflow starts automatically.

---

## 3. Let GitHub Actions + GoReleaser handle the release

Once the tag is pushed:

1. The **Release** workflow runs automatically.
2. GoReleaser:
   - Builds binaries for all configured OS/architecture combinations  
   - Packages them as archives named:

     ```text
     checksy_<version>_<os>_<arch>.tar.gz
     ```

   - Generates a checksum file  
   - Creates a GitHub Release for `vX.Y.Z` (if one does not already exist)  
   - Uploads all artifacts to that Release  

You can watch progress under:

```text
Actions → Release
```

---

## 4. Verify the release (optional)

After the workflow finishes:

1. Open:

   ```text
   https://github.com/notwillk/checksy/releases
   ```

2. Confirm the `vX.Y.Z` release has:
   - All platform archives  
   - The checksum file  
   - A successful workflow run  

---

## 5. Test installation (optional)

If using a curl installer script:

```bash
curl -fsSL https://raw.githubusercontent.com/notwillk/checksy/main/scripts/install.sh | bash
checksy --version
```

Confirm the installed version matches the release.

---

## Summary

1. Ensure `main` is up to date  
2. Run `./release.sh patch|minor|major`  
3. CI + GoReleaser handle everything once the tag is pushed  

**Release is complete.**

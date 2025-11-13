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

## 2. Create a version tag

Choose a version number and create an annotated tag of the form `vX.Y.Z`:

```bash
git tag -a vX.Y.Z -m "workspace-doctor vX.Y.Z"
git push origin vX.Y.Z
```

This push is the **only** required action to start a release.

---

## 3. Let GitHub Actions + GoReleaser handle the release

Once the tag is pushed:

1. The **Release** workflow runs automatically.
2. GoReleaser:
   - Builds binaries for all configured OS/architecture combinations  
   - Packages them as archives named:

     ```text
     workspace-doctor_<version>_<os>_<arch>.tar.gz
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
   https://github.com/notwillk/workspace-doctor/releases
   ```

2. Confirm the `vX.Y.Z` release has:
   - All platform archives  
   - The checksum file  
   - A successful workflow run  

---

## 5. Test installation (optional)

If using a curl installer script:

```bash
curl -fsSL https://raw.githubusercontent.com/notwillk/workspace-doctor/main/scripts/install.sh | bash
workspace-doctor --version
```

Confirm the installed version matches the release.

---

## Summary

1. Ensure `main` is up to date  
2. `git tag vX.Y.Z`  
3. `git push origin vX.Y.Z`  
4. CI + GoReleaser handle everything  

**Release is complete.**

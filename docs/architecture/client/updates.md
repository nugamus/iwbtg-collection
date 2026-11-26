# Updates: the client updates itself from CI

The Windows client checks the rolling `main-latest` prerelease and replaces its own `.exe`, so a
tester runs every green build of `main` without compiling (#1606). The module is `updater/`; the
workflow is `.github/workflows/main-latest.yml`.

## Publishing

`main-latest.yml` runs on `workflow_run` when CI completes successfully for a push to `main`, and
checks out the CI run's head SHA, so the published exe is exactly the tree CI passed. Its
concurrency group is `main-latest` with `cancel-in-progress`, so only the newest commit builds; an
older run is cancelled rather than allowed to publish after a newer one.

The build job reuses `nightly.yml`'s Windows packaging (the client, `showcase.pkg`, the licence
files) and uploads the zip and `sha256.txt`. The publish job, holding only `contents: write` and
the default `GITHUB_TOKEN`, moves the `main-latest` tag to the built commit and replaces three
assets on the prerelease:

- `main-latest-client-windows-x86_64.zip`, the packaged client;
- `sha256.txt`, `<hash>  <zip name>` for that zip;
- `build.json`, `{commit, date, version, prs}` where `prs` lists the titles of the PRs merged
  since the previous build (the compare API from the previous `build.json`'s commit, or the last
  20 commits on the first build).

The workflow embeds `IWBTG_GIT_SHA` and `IWBTG_BUILD_DATE` while compiling. `updater::BUILD_COMMIT`
and `BUILD_DATE` read them with `option_env!`; a build compiled on a developer's machine has
neither and never updates itself.

## Checking

On start and every 30 minutes **while on a menu** (`AppScreen::Menu` or the select list), a worker
thread reads the release through the public GitHub API with **no token** (the repository is
public; only a `User-Agent` is sent). The channel comes from `[updates] channel` in the settings
file:

- `main` (default): `/releases/tags/main-latest`;
- `releases`: `/releases/latest`. A release without `sha256.txt` and a Windows zip is not
  installable, and the Settings row says so; tagged releases do not publish checksums yet.

If `build.json` names a different commit from `BUILD_COMMIT`, the zip downloads in the background.
A check never runs in a run, and a verified download stops further checks so the staged exe
cannot be replaced under the dialog.

## Verifying and installing

The zip is checked against `sha256.txt` before anything is unpacked. A mismatch deletes the
download (`updater/net.rs` calls `cleanup`) and posts an update notice; an unverified download is
never run. Only the `iwbtg-client.exe` inside the zip is staged, in the system temp folder —
saves, settings, stats and packages are never in the plan.

On **Restart and update** (m5 B10, `MenuDialog.dc.html`), `install::swap` moves the running exe to
`iwbtg-client.exe.old`, copies the staged exe into its place, and spawns it with the same
arguments; the old process exits so two clients never share one settings file. If the copy fails
the old exe is moved back and an update notice says so — the running client still works. The next
start deletes the `.old` file (`install::remove_old_on_start`, called from the module's
`prepare`).

## The prompt and Settings

The B10 dialog shows the build's version and the `build.json` PR titles, with **Restart and
update**, **Later** and **What's new** (which shows up to eight titles). It only appears on a
menu: leaving for a run despawns it, and it comes back on the menu. **Later** dismisses it for the
session; the next start checks again.

The Settings → Updates row lives in `updater/ui.rs` (channel chips, **Check now**, the running
build's commit and date, and the last check's result). The settings screen itself is #53 and does
not exist yet, so `updater::Panel { open }` is what that screen will open; until then
`--updates-preview` draws the row for the screenshot check. The channel is persisted in
`settings.rs` as `[updates] channel = "main" | "releases"`.

## Tests and what stays manual

Unit tests cover the commit compare (`net::same_commit`), the checksum parse and verify
(`install`), reading the exe from a stored and a deflated zip, the swap plan against a temp
folder, and the settings channel. `cargo xtask check` and a real client build cover compilation.
The end-to-end user check (install the zip, merge a PR, see the prompt, restart into the new
commit) needs a merged CI build and cannot run on a workstation.

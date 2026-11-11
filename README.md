<p align="center">
  <img width="200px" src="crates-tauri/yaak-app-client/icons/scout.png" alt="Scout icon">
</p>

<h1 align="center">Scout</h1>

<p align="center">
  A fast, offline-first desktop API client for REST, GraphQL, SSE, WebSocket, and gRPC – built with Tauri, Rust, and React.
</p>

<p align="center">
  <a href="https://github.com/ForYouzi/scout/releases/latest">Download</a> ·
  <a href="DEVELOPMENT.md">Build from source</a> ·
  <a href="#license">License</a>
</p>

Scout is a community build of [Yaak](https://github.com/mountain-loop/yaak), kept in sync with upstream.
It ships its own name, icon, and app identifier, so it installs next to Yaak without sharing data.
Scout builds leave out Yaak's auto-updater and license check; new versions are published on the
[Releases page](https://github.com/ForYouzi/scout/releases).

## Install

Download the installer for your platform from [Releases](https://github.com/ForYouzi/scout/releases/latest).
Each release includes a `SHA256SUMS` file to verify downloads (`shasum -a 256 -c SHA256SUMS`).

| Platform              | File                                                                    |
| --------------------- | ----------------------------------------------------------------------- |
| macOS (Apple Silicon) | `Scout_<version>_macos_aarch64.dmg`                                     |
| macOS (Intel)         | `Scout_<version>_macos_x64.dmg`                                         |
| Windows (x64 / ARM64) | `Scout_<version>_windows_x64-setup.exe` / `..._windows_arm64-setup.exe` |
| Linux (x64 / ARM64)   | `Scout_<version>_linux_<x86_64\|aarch64>.deb`, `.rpm`, or `.AppImage`   |

Scout builds are not code-signed, so your OS will warn the first time you open them:

- **macOS:** after moving Scout to Applications, run `xattr -dr com.apple.quarantine /Applications/Scout.app`, or right-click the app and choose **Open**.
- **Windows:** on the SmartScreen prompt, choose **More info → Run anyway**.
- **Linux:** for the AppImage, run `chmod +x Scout_*.AppImage` first.

## Features

### 🌐 Work with any API

- Import collections from Postman, Insomnia, OpenAPI, Swagger, Bruno, or Curl.
- Send requests via REST, GraphQL, gRPC, WebSocket, or Server-Sent Events.
- Filter and inspect responses with JSONPath or XPath.

### 🔐 Stay secure

- Use OAuth 2.0, JWT, Basic Auth, or custom plugins for authentication.
- Secure sensitive values with encrypted secrets.
- Store secrets in your OS keychain.

### ☁️ Organize & collaborate

- Group requests into workspaces and nested folders.
- Use environment variables to switch between dev, staging, and prod.
- Mirror workspaces to your filesystem for versioning in Git or syncing with Dropbox.

### 🧩 Extend & customize

- Insert dynamic values like UUIDs or timestamps with template tags.
- Pick from built-in themes or build your own.
- Create plugins to extend authentication, template tags, or the UI.

## Releasing

Releases are built by [`.github/workflows/release-scout.yml`](.github/workflows/release-scout.yml).
Push a tag named `scout-v<version>` (for example `scout-v2026.11.0`, or `scout-v2026.11.0-beta.1` for a prerelease)
on a commit that is on `main`. The workflow builds macOS, Windows, and Linux installers, then publishes them with
`SHA256SUMS` as a GitHub Release. The workflow runs only for a pushed `scout-v*` tag: pull requests and pushes to
`main` run CI (`Lint and Test`), never release builds.

Upstream `v*` tags are ignored, so syncing from Yaak never triggers a Scout release.

GitHub can pause workflows on this fork when its Actions usage is high. The Actions tab then shows
"Workflows are not currently being run on this fork", and nothing runs: no CI on pull requests or `main`,
and a pushed `scout-v*` tag starts no release. A maintainer re-enables them from the Actions tab; the
API cannot. A tag pushed while workflows were paused has to be pushed again, or released under a new version.

## Syncing with upstream

```sh
git remote add upstream https://github.com/mountain-loop/yaak.git   # once
git fetch upstream
git switch main && git merge upstream/main
```

Scout-specific files are kept separate to keep merges small: the release workflow, `crates-tauri/yaak-app-client/tauri.scout.conf.json`, and `crates-tauri/yaak-app-client/icons/scout*`.

## Useful resources

- [Yaak documentation](https://yaak.app/docs) – applies to Scout too
- [Local development setup](DEVELOPMENT.md)
- [Scout releases](https://github.com/ForYouzi/scout/releases)

## License

Scout is based on Yaak and is distributed under the [MIT license](LICENSE), copyright (c) 2024 Yaak.
Scout builds are produced from source by this fork and are not official Yaak releases.

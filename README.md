# paper-bumper

Automated Minecraft plugin dependency updater and headless server smoke tester written in Rust.

Tracks PaperMC and Purpur releases, updates build descriptors (`gradle.properties`, `build.gradle.kts`, `pom.xml`, `paper-plugin.yml`), compiles the plugin, and boots an actual headless Paper/Purpur server in CI to verify plugin startup and runtime stability.

---

## Features

- **PaperMC & Purpur API Integration**: Queries `fill.papermc.io` and `api.purpurmc.org` for latest stable or experimental builds.
- **Universal Build Tool Support**: Auto-detects and updates Gradle (`build.gradle.kts`, `build.gradle`, `gradle.properties`) and Maven (`pom.xml`).
- **Plugin Descriptor Updating**: Synchronizes `api-version` inside `paper-plugin.yml` and `plugin.yml`.
- **Headless Server Smoke Test**:
  - Automatically downloads the exact Paper/Purpur server JAR for the target Minecraft release.
  - Generates optimized bootstrap configuration (`eula=true`, flat world, minimal tick time, no GUI).
  - Injects compiled plugin into `plugins/`.
  - Captures stdout/stderr in real-time, asserts clean `onEnable()` and absence of exceptions/stacktraces.
  - Sends diagnostic commands (`version`, `plugins`, custom plugin commands).
  - Gracefully stops the server and asserts clean `onDisable()`.
- **Automated Pull Requests**: Creates a git branch and opens a GitHub Pull Request with a full smoke test log report.

---

## GitHub Action Setup

Add `.github/workflows/paper-update.yml` to your plugin repository:

```yaml
name: Auto Update Minecraft Plugin

on:
  schedule:
    - cron: '0 0 * * *'
  workflow_dispatch:

jobs:
  update:
    runs-on: ubuntu-latest
    permissions:
      contents: write
      pull-requests: write

    steps:
      - name: Checkout repository
        uses: actions/checkout@v4

      - name: Set up Java
        uses: actions/setup-java@v4
        with:
          distribution: 'temurin'
          java-version: '21'

      - name: Run Paper Bumper
        uses: sqrilizz/paper-bumper@main
        with:
          software: 'paper'
          channel: 'STABLE'
          auto-pr: 'true'
          smoke-test: 'true'
          test-commands: 'version,plugins'
```

---

## Action Inputs

| Input | Description | Default |
|---|---|---|
| `project-dir` | Path to plugin project root | `.` |
| `software` | Server software (`paper` or `purpur`) | `paper` |
| `channel` | Release channel (`STABLE` or `EXPERIMENTAL`) | `STABLE` |
| `target-version` | Force a specific Minecraft version (e.g. `26.3`) | *(latest from API)* |
| `smoke-test` | Run headless server verification | `true` |
| `test-commands` | Comma-separated commands to execute during test | `version,plugins` |
| `timeout` | Server startup timeout in seconds | `120` |
| `auto-pr` | Open a Pull Request when checks pass | `true` |
| `github-token` | GitHub token for git push and PR creation | `${{ github.token }}` |

---

## CLI Usage (Local)

Build binary:
```bash
cargo build --release
```

Check for new Minecraft version:
```bash
./target/release/paper-bumper check --software paper
```

Apply version bump to project files:
```bash
./target/release/paper-bumper bump --software paper
```

Run standalone smoke test against an existing server JAR:
```bash
./target/release/paper-bumper smoke-test \
  --server-jar ./paper.jar \
  --plugin-jar ./build/libs/MyPlugin.jar \
  --commands "version,plugins"
```

Run full pipeline (check -> bump -> compile -> smoke test):
```bash
./target/release/paper-bumper run --software paper
```

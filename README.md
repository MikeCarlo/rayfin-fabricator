<div align="center">
  <img src="./logo.png" alt="Fabricator logo" width="120" />

  <h1>Fabricator</h1>

  <p><strong>The all-in-one workbench for building Rayfin apps — chat to build, preview inline, and ship to Microsoft Fabric, all in one window. No CLI wrangling, no new account: just your GitHub Copilot sign-in.</strong></p>

  <p>
    <a href="https://github.com/spatney/rayfin-fabricator/releases/latest"><img alt="Download Fabricator" src="https://img.shields.io/badge/Download-Fabricator-0078D4?style=for-the-badge" /></a>
    <a href="https://spatney.github.io/rayfin-fabricator/docs"><img alt="Read the docs" src="https://img.shields.io/badge/Read-the%20docs-35A3EA?style=for-the-badge" /></a>
    <a href="https://www.youtube.com/watch?v=4T1oQmT8Bks"><img alt="Watch the intro video on YouTube" src="https://img.shields.io/badge/Watch-the%20intro-FF0000?style=for-the-badge&amp;logo=youtube&amp;logoColor=white" /></a>
  </p>

  <p>
    <a href="./LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg" /></a>
    <img alt="Platform: Windows, macOS, and Linux" src="https://img.shields.io/badge/platform-Windows%20%C2%B7%20macOS%20%C2%B7%20Linux-0078D4.svg" />
    <img alt="Runs in the browser" src="https://img.shields.io/badge/runs%20in-your%20browser-24C8DB.svg" />
  </p>
</div>

<div align="center">
  <a href="https://spatney.github.io/rayfin-fabricator/">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="./website/public/screenshots/workbench.webp" />
      <source media="(prefers-color-scheme: light)" srcset="./website/public/screenshots/workbench.light.webp" />
      <img src="./website/public/screenshots/workbench.webp" alt="The Fabricator workbench: the chat on the left shows a finished Copilot turn that built an expense tracker, and the preview on the right shows the published app." width="100%" />
    </picture>
  </a>
  <p><sub>Build, preview, and ship — all in one window.</sub></p>
</div>

> **Personal project disclaimer**
> Fabricator is a personal project built by Sachin Patney in his own free time. The author works at Microsoft, but this is not a Microsoft product and is not affiliated with, endorsed by, sponsored by, or supported by Microsoft.

Building a Rayfin app usually means living in your terminal: scaffold with one CLI, prompt the Copilot CLI, run `rayfin up` to deploy, wrangle git, flip to a browser to check it, repeat. Fabricator folds all of that into one local app that opens in your browser. Nothing to install: download one executable and run it.

You chat, the app gets built, you watch it come together inline, and you manage every published app from one panel. It runs on the **GitHub Copilot account you already have** — nothing new to sign up for.

**📖 Documentation: [spatney.github.io/rayfin-fabricator](https://spatney.github.io/rayfin-fabricator/)** — installation, a first-app walkthrough, guides for every part of the app, team workspaces, and troubleshooting.

### New to Rayfin?

Rayfin is Microsoft's **Backend-as-a-Service for the agentic era**. You define your data model with TypeScript decorators and the platform provisions and manages the database, authentication, data APIs, storage, and hosting for you — all on Microsoft Fabric, with enterprise-grade governance built in. Learn more at [rayfin.ai](https://rayfin.ai) and [microsoft/rayfin](https://github.com/microsoft/rayfin).

## Everything in one window

1. **[Chat to build.](https://spatney.github.io/rayfin-fabricator/docs/build/chat)** Describe what you want in plain English. The built-in GitHub Copilot agent writes and edits the project files for you. Grok CLI and Claude Code can do the same job once you install them and pick them on the chat composer. Git quietly snapshots every change so you can diff and roll back.
2. **[See it as it's built.](https://spatney.github.io/rayfin-fabricator/docs/build/preview)** Watch the app in a live inline preview, point at anything in it with [Point & Edit](https://spatney.github.io/rayfin-fabricator/docs/build/touch-up), or clip and draw on a screenshot of it, to say what to change, and browse every file, its history and the data model.
3. **[Publish with a click.](https://spatney.github.io/rayfin-fabricator/docs/ship/deploy)** Fabricator runs `rayfin up` for you to publish your app to Microsoft Fabric when you choose (or [after each successful chat turn](https://spatney.github.io/rayfin-fabricator/docs/ship/deploy#publish-after-each-chat-turn)), and lets you [share the app](https://spatney.github.io/rayfin-fabricator/docs/ship/share) with people in your tenant.
4. **[Harden it.](https://spatney.github.io/rayfin-fabricator/docs/ship/advisor)** The Advisor grades your app's health with instant checks and an on-demand, read-only Copilot review, and every finding comes with a one-click fix.
5. **[Build with your team](https://spatney.github.io/rayfin-fabricator/docs/team)** *(experimental)*. Team workspaces keep your team's apps in a private GitHub repository, give everyone a personal preview, and publish to Fabric through a pipeline.
6. **[Use your own API keys](https://spatney.github.io/rayfin-fabricator/docs/build/your-own-api-keys)**. Add Claude, GPT, Grok and Gemini models on your own Anthropic, OpenAI, xAI or Google key to the model pickers, next to your Copilot models, in chat and the Advisor.
7. **[Migrate a Power BI report](https://spatney.github.io/rayfin-fabricator/docs/build/migrate-power-bi)** *(experimental)*. Turn a report into an app that reads its semantic model live, while Copilot rebuilds its pages, visuals and filters one page at a time with [Graphein](https://github.com/spatney/graphein) charts.
8. **[Develop on this computer](https://spatney.github.io/rayfin-fabricator/docs/build/local-development)**. Your apps and their backend run on this computer while you build, on the bundled [rayfin-local](https://github.com/spatney/rayfin-local), and you publish to Microsoft Fabric when you choose.

## Download

Fabricator is one file. It listens on `127.0.0.1` only, opens your browser, and serves the app from that process. There is no installer and nothing to compile on your PC.

You need the [GitHub Copilot CLI](https://docs.github.com/en/copilot/how-tos/copilot-cli) (or another agent you pick) and the [Rayfin CLI](https://github.com/microsoft/rayfin) on your PATH. Fabricator does not need the Visual Studio C++ compiler, WebView2, or an installer.

> **[⬇️ Download the latest release](https://github.com/spatney/rayfin-fabricator/releases/latest)**

Every push also uploads the executables as a workflow artifact on the [Actions](https://github.com/spatney/rayfin-fabricator/actions) run, before a release exists. A tag named `v*` (for example `v1.55.0`) attaches the same files to a GitHub Release.

- **Windows:** download `fabricator.exe` and double-click it, or run it from a terminal. A console window prints the address, for example `http://127.0.0.1:5173/?token=…`, and your browser opens there. If SmartScreen warns about a new file, choose **More info → Run anyway**.
- **macOS:** download `fabricator-macos`, `chmod +x fabricator-macos`, and run it. The first launch may need a right-click → Open, because the file is not notarized.
- **Linux:** download `fabricator-linux`, `chmod +x fabricator-linux`, and run it. The local Rayfin backend (`rayfin-local`) is bundled for Windows and macOS. On Linux, set `FABRICATOR_RAYFIN_LOCAL_BIN` to a `rayfin-local` binary you built, or projects preview through the published app instead.

Close the console window (or press Ctrl+C) to quit. A second launch while one is running opens the browser again instead of starting another server.

`5173` is the first port it tries. If something else is using it, including a Rayfin preview, Fabricator takes the next free port and prints that address.

See the [install guide](https://spatney.github.io/rayfin-fabricator/docs/start/install) and [Build your first app](https://spatney.github.io/rayfin-fabricator/docs/start/first-app) for the walkthrough. Settings → Check for updates downloads a newer executable from the latest GitHub Release and restarts into it.

## Grok CLI and Claude Code

GitHub Copilot ships with Fabricator and stays the default chat agent. Grok CLI and Claude Code are optional. Fabricator does not bundle them. Install one (or both) on this computer, then pick it on the chat composer, to the left of the model picker. Each project keeps its own choice.

This is separate from [using your own API keys](https://spatney.github.io/rayfin-fabricator/docs/build/your-own-api-keys). That setting adds Claude, GPT, Grok, and Gemini models to Copilot's model picker. Grok CLI and Claude Code are their own agents, signed in on this computer.

If `grok` or `claude` is missing from your PATH, chat shows the install steps below when you pick that agent.

### Grok CLI

**Windows (PowerShell):**

```powershell
irm https://x.ai/cli/install.ps1 | iex
```

**macOS:**

```bash
curl -fsSL https://x.ai/cli/install.sh | bash
```

**Either platform, with npm:**

```bash
npm install -g @xai-official/grok
```

Run `grok` and sign in in the browser, or set `XAI_API_KEY`. `grok --version` confirms it is on your PATH. Full docs: [Grok Build](https://x.ai/docs/build/overview).

### Claude Code

**Windows (PowerShell):**

```powershell
irm https://claude.ai/install.ps1 | iex
```

**macOS:**

```bash
curl -fsSL https://claude.ai/install.sh | bash
```

**npm**, on either platform:

```bash
npm install -g @anthropic-ai/claude-code
```

The [Claude Code setup guide](https://code.claude.com/docs/en/setup) also covers Homebrew, WinGet, and sign-in. Run `claude` and sign in, or set `ANTHROPIC_API_KEY`. `claude --version` confirms it is on your PATH.

In Agent or Autopilot, the agent you picked can edit the project. Plan mode answers without applying edits.

## Documentation

| Start here | Guides | When something goes wrong |
| --- | --- | --- |
| [What is Fabricator?](https://spatney.github.io/rayfin-fabricator/docs) | [Chat with Copilot](https://spatney.github.io/rayfin-fabricator/docs/build/chat) | [Troubleshooting](https://spatney.github.io/rayfin-fabricator/docs/troubleshooting) |
| [Install Fabricator](https://spatney.github.io/rayfin-fabricator/docs/start/install) | [Publish to Microsoft Fabric](https://spatney.github.io/rayfin-fabricator/docs/ship/deploy) | [Report a problem](https://spatney.github.io/rayfin-fabricator/docs/troubleshooting/report-a-problem) |
| [Set up and sign in](https://spatney.github.io/rayfin-fabricator/docs/start/setup) | [Check app health with the Advisor](https://spatney.github.io/rayfin-fabricator/docs/ship/advisor) | [FAQ](https://spatney.github.io/rayfin-fabricator/docs/reference/faq) |
| [Build your first app](https://spatney.github.io/rayfin-fabricator/docs/start/first-app) | [Team workspaces](https://spatney.github.io/rayfin-fabricator/docs/team) | [Privacy and telemetry](https://spatney.github.io/rayfin-fabricator/docs/reference/privacy) |

The site's source is in [`website/`](./website); every page has an **Edit this page** link.

## Build from source

You'll need Node.js 24 LTS (22.13+ also works), Rust stable, and Git. Windows CI builds with the Visual Studio compiler that GitHub's `windows-latest` runner already has. You do not need that compiler to *run* a downloaded `fabricator.exe`.

```bash
git clone https://github.com/spatney/rayfin-fabricator.git
cd rayfin-fabricator
npm install
npm run rayfin-local:fetch
npm run build:renderer
cargo run --manifest-path src-tauri/Cargo.toml --bin fabricator
```

`npm install` runs `scripts/rayfin-local/fetch.mjs`, which downloads the pinned [rayfin-local](https://github.com/spatney/rayfin-local) release (Windows and macOS; Linux warns and skips). The server build embeds that binary, and also looks beside the executable and at `FABRICATOR_RAYFIN_LOCAL_BIN`.

For UI work, leave the server running and start the renderer dev server. It stays on port 1420 and proxies `/api` and `/ws` to the server (the address is in `src-tauri/.fabricator-dev.json`, which the server writes on each launch):

```bash
npm run dev:renderer
```

`npm run build` builds the renderer and then `fabricator` in release mode. See [`CONTRIBUTING.md`](./CONTRIBUTING.md) for scripts, the project layout, and the docs site.

## Telemetry & privacy

Telemetry is optional and stays off unless a connection string is built in; local development builds send nothing. Official releases send coarse usage events with a hashed email and your sign-in domain — never your code, prompts or apps. Details are in [Privacy and telemetry](https://spatney.github.io/rayfin-fabricator/docs/reference/privacy); maintainer provisioning lives in [`docs/DEPLOY.md`](./docs/DEPLOY.md).

## Contributing

Contributions are welcome. Read [`CONTRIBUTING.md`](./CONTRIBUTING.md) and follow the [`CODE_OF_CONDUCT.md`](./CODE_OF_CONDUCT.md).

## Security

Report security issues per [`SECURITY.md`](./SECURITY.md). Please don't open public issues for sensitive reports.

## License

Fabricator is released under the [MIT License](./LICENSE).

## Disclaimer

This is a personal project built by [Sachin Patney](https://github.com/spatney) in his own free time. The author works at Microsoft, but Fabricator is not a Microsoft product and is not affiliated with, endorsed by, sponsored by, or supported by Microsoft.

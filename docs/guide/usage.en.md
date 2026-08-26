# WebLaunch Usage Guide

WebLaunch launches and hosts any web app as a native desktop app. This guide covers everyday usage: adding apps, launching/stopping, desktop shortcuts, lifecycle hooks, and the command-authorization bridge.

## Contents

1. [Install](#install)
2. [Add an app](#add-an-app)
3. [Launch / stop / activate](#launch--stop--activate)
4. [Desktop shortcuts](#desktop-shortcuts)
5. [Lifecycle hooks](#lifecycle-hooks)
6. [Command-authorization bridge](#command-authorization-bridge)
7. [Management console](#management-console)
8. [CLI reference](#cli-reference)

---

## Install

1. Download the installer from [Releases](https://github.com/re-ITRT/weblaunch/releases) (`WebLaunch_x.x.x_x64-setup.exe`).
2. Run the installer.
3. On first launch the daemon starts automatically and the **management console** opens at `http://127.0.0.1:3070`.

> The install directory contains `weblaunch.exe` (both CLI and daemon — a single binary).

## Add an app

**Console**: click **「+ Add App」**, enter a name + URL, save.

**CLI**:

```bash
weblaunch addweb -url https://example.com -name "Example"
```

> Fixed port `3070`, no auth. Apps and status are visible in both console and CLI.

## Launch / stop / activate

Each app card in the console has **Launch / Activate / Terminate** buttons:

- **Launch**: opens that app's own window (independent taskbar icon).
- **Activate**: bring back a backgrounded window.
- **Terminate**: close the window and stop.

CLI:

```bash
weblaunch app launch <id>
weblaunch app activate <id>
weblaunch app terminate <id>
weblaunch app status <id>
```

## Desktop shortcuts

Every app can create a desktop shortcut. **The icon is bound to the app itself** (fetches the app's favicon and converts it to .ico).

Console: app card → **「Desktop Shortcut」** → choose an icon source (default / local .ico / auto from webpage) → create.

CLI:

```bash
weblaunch app shortcut <id>
```

Double-clicking the shortcut opens the app directly (`--launch <id>`): it auto-starts the daemon and opens the window.

## Lifecycle hooks

**Pre-launch hooks**: run before the app window appears, for environment preparation (starting local services, SSH tunnels, databases).
**Post-exit hooks**: run after the app closes, for cleanup.

Type commands directly into the **Pre-launch hook / Post-exit hook** multiline fields when editing an app. You can paste **bat code** directly (multi-line / `@echo off`); WebLaunch auto-writes it to a .bat and runs it.

> Hooks run in `cmd` by default. To run in WSL, type the full command, e.g.:
> `wsl -d Ubuntu-24.04 --cd ~ -e /home/user/myapp.sh`

## Command-authorization bridge

WebLaunch injects `window.webdesk.exec(command)` into every app window. **Web pages can safely request local command execution** (e.g. clicking a file to open it in the host OS).

Security model:

- First request for a command → an **approval dialog** shows the command to run.
- Optionally check **「Don't ask again」** → remembers the grant (per app + command).
- User clicks **Allow** → run; **Deny** → refuse.
- Approved commands run directly on subsequent calls.

> Grants are stored in `%APPDATA%/WebDesk/auth/grants.json`.

Web page example:

```js
// Open a file in the host OS (authorized)
const result = await window.webdesk.exec('explorer.exe "C:\\path\\file.txt"');
if (result.ok) console.log("executed");
```

## Management console

- **Apps** tab: grid of all apps; each card has Launch / Terminate / Edit / Shortcut / Delete.
- **Settings** tab: language switch (Chinese / English).
- Operations report via **auto-dismissing toasts** (no blocking dialogs).
- Deleting an app / removing a shortcut asks for confirmation.

## CLI reference

```bash
weblaunch addweb -url <url> -name <name> [-hook "cmd"] [-hook_exit "cmd"]  # add app
weblaunch app list          # list apps
weblaunch app get <id>      # show details
weblaunch app launch <id>   # launch
weblaunch app stop <id>     # stop
weblaunch app activate <id> # activate
weblaunch app status <id>   # status
weblaunch app shortcut <id> # desktop shortcut
weblaunch app remove <id>   # remove
weblaunch console           # open console
weblaunch status            # platform status
weblaunch version           # version
```

> Data (app configs / icons / grants / logs) lives under `%APPDATA%/WebDesk/` (Windows).

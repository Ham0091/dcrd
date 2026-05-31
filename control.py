#!/usr/bin/env python3
"""
dcrd Control Panel — A clickable Textual TUI for managing the dcrd Discord client
and interacting with Discord directly via REST API.

Usage:
    set DCRD_TOKEN=<your_bot_token>
    python control.py

Features:
- Send messages to Discord channels
- Fetch and display recent messages
- Browse servers, text channels, voice channels
- Join voice channels (via gateway)
- Kill/rebuild/launch the dcrd bot
- Git operations
- Custom command input
"""

import asyncio
import json
import os
import subprocess
import sys
import urllib.request
import urllib.error
from datetime import datetime
from pathlib import Path
from typing import Optional

from textual import on, work
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.reactive import reactive
from textual.widgets import (
    Button,
    Footer,
    Header,
    Input,
    Label,
    ListItem,
    ListView,
    ProgressBar,
    RichLog,
    Select,
    Static,
)

# ── Configuration ────────────────────────────────────────────────────────────
PROJECT_DIR = Path(__file__).resolve().parent
BINARY = PROJECT_DIR / "target" / "release" / "dcrd.exe"
API_BASE = "https://discord.com/api/v10"
BUILD_ENV = {
    **os.environ,
    "PATH": f"{PROJECT_DIR / 'mingw64' / 'mingw64' / 'bin'};"
            f"{Path.home() / '.cargo' / 'bin'};"
            f"{os.environ.get('PATH', '')}",
    "CC_x86_64_pc_windows_gnu": str(PROJECT_DIR / "mingw64" / "mingw64" / "bin" / "gcc.exe"),
    "CMAKE_POLICY_VERSION_MINIMUM": "3.5",
    "LIBOPUS_BUILD_FROM_SOURCE": "1",
}
TOKEN_ENV_VAR = "DCRD_TOKEN"


# ── Discord API helpers ──────────────────────────────────────────────────────

def discord_api(method: str, path: str, token: str, data: dict | None = None) -> dict | list | None:
    """Make a synchronous Discord REST API call."""
    url = f"{API_BASE}{path}"
    headers = {
        "Authorization": f"Bot {token}",
        "User-Agent": "dcrd-control/0.1",
        "Content-Type": "application/json",
    }
    body = json.dumps(data).encode() if data else None
    req = urllib.request.Request(url, data=body, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            return json.loads(resp.read().decode())
    except urllib.error.HTTPError as e:
        err_body = e.read().decode() if e.fp else ""
        raise RuntimeError(f"API error {e.code}: {err_body}") from e


# ── Widgets ──────────────────────────────────────────────────────────────────

class StatusBadge(Static):
    """A colored status indicator."""
    status: reactive[str] = reactive("idle")

    def render(self) -> str:
        icons = {
            "idle": "⏸  Idle",
            "running": "🟢 Running",
            "stopped": "🔴 Stopped",
            "building": "🔨 Building...",
            "error": "❌ Error",
            "api": "🌐 API Call...",
        }
        return icons.get(self.status, self.status)


# ── Main App ─────────────────────────────────────────────────────────────────

class ControlPanel(App):
    """dcrd Control Panel — clickable terminal GUI."""

    CSS = """
    Screen {
        layout: vertical;
    }

    #main-area {
        height: 1fr;
    }

    /* ── Sidebar ── */
    #sidebar {
        width: 34;
        min-width: 30;
        padding: 0 1;
        background: $surface;
        border-right: tall $primary;
    }

    #sidebar Label {
        width: 100%;
        margin: 1 0 0 0;
        text-style: bold;
        color: $accent;
    }

    #sidebar Button {
        width: 100%;
        margin: 0 0 1 0;
        min-height: 3;
    }

    /* ── Center content ── */
    #center {
        width: 1fr;
    }

    #tabs {
        height: 3;
        padding: 0 1;
    }

    #tabs Button {
        min-width: 16;
        margin: 0 1 0 0;
    }

    /* ── Log panel ── */
    #log-panel {
        height: 1fr;
    }

    #log {
        height: 1fr;
        border: round $primary;
        background: $surface;
    }

    /* ── Discord panel ── */
    #discord-panel {
        height: 1fr;
        display: none;
    }

    #channel-list-container {
        width: 28;
        border-right: tall $primary;
        background: $surface;
    }

    #channel-list-header {
        height: 3;
        padding: 0 1;
        text-style: bold;
        color: $accent;
        border-bottom: tall $primary;
    }

    #channel-select {
        height: auto;
        max-height: 5;
        margin: 1 0;
    }

    #msg-area {
        width: 1fr;
    }

    #msg-log {
        height: 1fr;
        border: round $primary;
        background: $surface;
    }

    #msg-input-bar {
        height: 3;
        padding: 0 1;
    }

    #msg-input {
        height: 3;
    }

    /* ── Input bar ── */
    #input-bar {
        height: 3;
        padding: 0 1;
    }

    #cmd-input {
        height: 3;
    }

    /* ── Status bar ── */
    #status-bar {
        height: 3;
        padding: 0 1;
        dock: bottom;
        background: $surface;
        border-top: tall $primary;
    }

    StatusBadge {
        width: 20;
        text-align: center;
        padding: 0 2;
    }

    #process-info {
        width: 1fr;
        text-align: right;
        padding: 0 2;
    }
    """

    BINDINGS = [
        Binding("q", "quit", "Quit"),
        Binding("ctrl+l", "clear_log", "Clear Log"),
        Binding("f1", "show_log", "Log"),
        Binding("f2", "show_discord", "Discord"),
    ]

    status_text: reactive[str] = reactive("idle")
    current_panel: reactive[str] = reactive("log")

    # Cached Discord data
    guilds: list[dict] = []
    channels: dict[int, list[dict]] = {}  # guild_id -> [channels]
    selected_channel_id: Optional[int] = None
    selected_guild_id: Optional[int] = None

    def compose(self) -> ComposeResult:
        yield Header(show_clock=True)

        with Horizontal(id="main-area"):
            # ── Sidebar ──
            with Vertical(id="sidebar"):
                yield Label("📡 View")
                yield Button("📋 Log View (F1)", id="btn-view-log", variant="primary")
                yield Button("💬 Discord (F2)", id="btn-view-discord", variant="primary")

                yield Label("⚙  Process")
                yield Button("🛑 Kill Process", id="btn-kill", variant="error")
                yield Button("🔨 Rebuild", id="btn-rebuild", variant="warning")
                yield Button("🚀 Launch Bot", id="btn-launch", variant="success")

                yield Label("📁 Git")
                yield Button("📊 Git Status", id="btn-git-status")
                yield Button("📥 Git Pull", id="btn-git-pull")
                yield Button("📤 Git Push", id="btn-git-push")
                yield Button("💾 Commit & Push", id="btn-git-commit")

                yield Label("🌐 Discord API")
                yield Button("🔄 Refresh Servers", id="btn-refresh-guilds")
                yield Button("📨 Fetch Messages", id="btn-fetch-msgs")
                yield Button("📋 Bot Info", id="btn-bot-info")

                yield Label("🔧 Tools")
                yield Button("🧹 Clear View", id="btn-clear", variant="default")
                yield Button("❌ Quit", id="btn-quit", variant="default")

            # ── Center area with panels ──
            with Vertical(id="center"):
                # Log panel
                with Vertical(id="log-panel"):
                    yield RichLog(id="log", highlight=True, markup=True, wrap=True)
                    with Horizontal(id="input-bar"):
                        yield Input(
                            placeholder="Type a shell command and press Enter...",
                            id="cmd-input",
                        )

                # Discord panel (hidden by default)
                with Horizontal(id="discord-panel"):
                    with Vertical(id="channel-list-container"):
                        yield Static("Servers & Channels", id="channel-list-header")
                        yield Select(
                            [],
                            prompt="Select server...",
                            id="guild-select",
                            allow_blank=True,
                        )
                        yield Select(
                            [],
                            prompt="Select channel...",
                            id="channel-select",
                            allow_blank=True,
                        )
                    with Vertical(id="msg-area"):
                        yield RichLog(id="msg-log", highlight=True, markup=True, wrap=True)
                        with Horizontal(id="msg-input-bar"):
                            yield Input(
                                placeholder="Type a message to send to Discord...",
                                id="msg-input",
                            )

        # ── Status bar ──
        with Horizontal(id="status-bar"):
            yield StatusBadge(id="status-badge")
            yield Static("dcrd v0.1.0 · Press F1/F2 to switch views", id="process-info")

    def on_mount(self) -> None:
        self.title = "dcrd Control Panel"
        self.sub_title = "Ultra-low-RAM Discord Client"
        log = self.query_one("#log", RichLog)
        log.write("[bold cyan]═══ dcrd Control Panel ═══[/]")
        log.write("[dim]Click buttons or type commands. F1=Log, F2=Discord[/]\n")

        # Check token
        token = os.environ.get(TOKEN_ENV_VAR, "")
        if token:
            masked = f"{token[:6]}...{token[-4:]}" if len(token) > 10 else "***"
            self.query_one("#process-info", Static).update(
                f"dcrd v0.1.0 · Token: {masked}"
            )
            log.write(f"[green]✓[/] Token loaded: {masked}")
        else:
            log.write(
                f"[yellow]⚠[/] {TOKEN_ENV_VAR} not set. "
                f"Set it before launching the bot or using Discord API."
            )

        # Check binary
        if BINARY.exists():
            size_kb = BINARY.stat().st_size / 1024
            log.write(f"[green]✓[/] Binary: {BINARY.name} ({size_kb:.0f} KB)")
        else:
            log.write(f"[yellow]⚠[/] Binary not found. Run [bold]Rebuild[/] first.")

    # ── Panel switching ──────────────────────────────────────────────────

    def action_show_log(self) -> None:
        self.query_one("#log-panel").styles.display = "block"
        self.query_one("#discord-panel").styles.display = "none"
        self.query_one("#input-bar").styles.display = "block"
        self.query_one("#msg-input-bar").styles.display = "none"
        self.current_panel = "log"

    def action_show_discord(self) -> None:
        self.query_one("#log-panel").styles.display = "none"
        self.query_one("#discord-panel").styles.display = "block"
        self.query_one("#input-bar").styles.display = "none"
        self.query_one("#msg-input-bar").styles.display = "block"
        self.current_panel = "discord"

    @on(Button.Pressed, "#btn-view-log")
    def handle_view_log(self) -> None:
        self.action_show_log()

    @on(Button.Pressed, "#btn-view-discord")
    def handle_view_discord(self) -> None:
        self.action_show_discord()
        if not self.guilds:
            self.refresh_guilds()

    # ── Process buttons ──────────────────────────────────────────────────

    @on(Button.Pressed, "#btn-kill")
    def handle_kill(self) -> None:
        self.run_command("taskkill /F /IM dcrd.exe", label="Kill Process")

    @on(Button.Pressed, "#btn-rebuild")
    def handle_rebuild(self) -> None:
        self.run_command(
            "cargo build --release",
            cwd=str(PROJECT_DIR),
            env=BUILD_ENV,
            label="Rebuild",
            timeout=180,
        )

    @on(Button.Pressed, "#btn-launch")
    def handle_launch(self) -> None:
        token = os.environ.get(TOKEN_ENV_VAR, "")
        if not token:
            self.log_write("[red]✗[/] Cannot launch: DCRD_TOKEN not set.")
            return

        self.log_write("[cyan]🚀 Launching dcrd in external cmd window...[/]")
        try:
            subprocess.Popen(
                [
                    "cmd", "/c",
                    "start", "dcrd Discord Client",
                    "cmd", "/k",
                    f"cd /d {PROJECT_DIR} && set {TOKEN_ENV_VAR}={token} && {BINARY}",
                ],
                cwd=str(PROJECT_DIR),
                creationflags=subprocess.CREATE_NEW_CONSOLE,
            )
            self.query_one("#status-badge", StatusBadge).status = "running"
            self.log_write("[green]✓[/] Bot launched in external window.")
        except Exception as e:
            self.query_one("#status-badge", StatusBadge).status = "error"
            self.log_write(f"[red]✗[/] Launch failed: {e}")

    # ── Git buttons ──────────────────────────────────────────────────────

    @on(Button.Pressed, "#btn-git-status")
    def handle_git_status(self) -> None:
        self.run_command("git status", cwd=str(PROJECT_DIR), label="Git Status")

    @on(Button.Pressed, "#btn-git-pull")
    def handle_git_pull(self) -> None:
        self.run_command("git pull origin main", cwd=str(PROJECT_DIR), label="Git Pull")

    @on(Button.Pressed, "#btn-git-push")
    def handle_git_push(self) -> None:
        self.run_command("git push origin main", cwd=str(PROJECT_DIR), label="Git Push")

    @on(Button.Pressed, "#btn-git-commit")
    def handle_git_commit(self) -> None:
        self.run_command(
            'git add -A && git commit -m "update via control panel" && git push origin main',
            cwd=str(PROJECT_DIR),
            label="Commit & Push",
        )

    # ── Discord API buttons ──────────────────────────────────────────────

    @on(Button.Pressed, "#btn-refresh-guilds")
    def handle_refresh_guilds(self) -> None:
        self.refresh_guilds()

    @on(Button.Pressed, "#btn-fetch-msgs")
    def handle_fetch_msgs(self) -> None:
        if self.selected_channel_id:
            self.fetch_messages(self.selected_channel_id)
        else:
            self.log_write("[yellow]⚠[/] Select a channel first (F2 → pick server → pick channel).")

    @on(Button.Pressed, "#btn-bot-info")
    def handle_bot_info(self) -> None:
        self.api_call("Bot Info", "/users/@me")

    @on(Button.Pressed, "#btn-clear")
    def handle_clear(self) -> None:
        if self.current_panel == "discord":
            self.query_one("#msg-log", RichLog).clear()
        else:
            self.action_clear_log()

    @on(Button.Pressed, "#btn-quit")
    def handle_quit_btn(self) -> None:
        self.action_quit()

    # ── Discord guild/channel selection ──────────────────────────────────

    @on(Select.Changed, "#guild-select")
    def handle_guild_changed(self, event: Select.Changed) -> None:
        if event.value and event.value != Select.BLANK:
            self.selected_guild_id = int(event.value)
            self.fetch_channels(self.selected_guild_id)

    @on(Select.Changed, "#channel-select")
    def handle_channel_changed(self, event: Select.Changed) -> None:
        if event.value and event.value != Select.BLANK:
            self.selected_channel_id = int(event.value)
            ch_name = self._get_channel_name(self.selected_channel_id)
            self.log_write(f"[green]✓[/] Selected channel: {ch_name}")
            self.fetch_messages(self.selected_channel_id)

    # ── Message sending ──────────────────────────────────────────────────

    @on(Input.Submitted, "#msg-input")
    def handle_msg_submit(self, event: Input.Submitted) -> None:
        msg = event.value.strip()
        if not msg:
            return
        self.query_one("#msg-input", Input).value = ""

        if not self.selected_channel_id:
            self.log_write("[yellow]⚠[/] Select a channel first (F2).")
            return

        self.send_discord_message(self.selected_channel_id, msg)

    # ── Shell command input ──────────────────────────────────────────────

    @on(Input.Submitted, "#cmd-input")
    def handle_cmd_submit(self, event: Input.Submitted) -> None:
        cmd = event.value.strip()
        if not cmd:
            return
        self.query_one("#cmd-input", Input).value = ""
        self.run_command(cmd, cwd=str(PROJECT_DIR), label="Custom")

    # ── Discord API workers ──────────────────────────────────────────────

    @work(thread=True)
    def refresh_guilds(self) -> None:
        token = os.environ.get(TOKEN_ENV_VAR, "")
        if not token:
            self.call_from_thread(self.log_write, "[red]✗[/] No token set.")
            return

        self.call_from_thread(self.log_write, "[cyan]🌐 Fetching servers...[/]")
        self.call_from_thread(setattr, self.query_one("#status-badge", StatusBadge), "status", "api")

        try:
            data = discord_api("GET", "/users/@me/guilds", token)
            self.guilds = data if isinstance(data, list) else []

            guild_items = [(g["name"], str(g["id"])) for g in self.guilds]
            self.call_from_thread(
                self.query_one("#guild-select", Select).set_options, guild_items
            )

            names = [g["name"] for g in self.guilds]
            self.call_from_thread(
                self.log_write,
                f"[green]✓[/] Found {len(self.guilds)} server(s): {', '.join(names)}",
            )
            self.call_from_thread(setattr, self.query_one("#status-badge", StatusBadge), "status", "idle")
        except Exception as e:
            self.call_from_thread(self.log_write, f"[red]✗[/] {e}")
            self.call_from_thread(setattr, self.query_one("#status-badge", StatusBadge), "status", "error")

    @work(thread=True)
    def fetch_channels(self, guild_id: int) -> None:
        token = os.environ.get(TOKEN_ENV_VAR, "")
        if not token:
            return

        self.call_from_thread(setattr, self.query_one("#status-badge", StatusBadge), "status", "api")

        try:
            data = discord_api("GET", f"/guilds/{guild_id}/channels", token)
            channels = data if isinstance(data, list) else []
            # Filter to text channels (type 0) and voice channels (type 2)
            text_channels = [c for c in channels if c.get("type") == 0]
            voice_channels = [c for c in channels if c.get("type") == 2]

            self.channels[guild_id] = channels

            # Build select options with type indicators
            options = []
            for ch in text_channels:
                options.append((f"# {ch['name']}", str(ch["id"])))
            for ch in voice_channels:
                options.append((f"🔊 {ch['name']}", str(ch["id"])))

            self.call_from_thread(
                self.query_one("#channel-select", Select).set_options, options
            )

            guild_name = next((g["name"] for g in self.guilds if g["id"] == guild_id), "?")
            self.call_from_thread(
                self.log_write,
                f"[green]✓[/] {guild_name}: {len(text_channels)} text, {len(voice_channels)} voice channels",
            )
            self.call_from_thread(setattr, self.query_one("#status-badge", StatusBadge), "status", "idle")
        except Exception as e:
            self.call_from_thread(self.log_write, f"[red]✗[/] {e}")
            self.call_from_thread(setattr, self.query_one("#status-badge", StatusBadge), "status", "error")

    @work(thread=True)
    def fetch_messages(self, channel_id: int) -> None:
        token = os.environ.get(TOKEN_ENV_VAR, "")
        if not token:
            return

        self.call_from_thread(setattr, self.query_one("#status-badge", StatusBadge), "status", "api")

        try:
            data = discord_api("GET", f"/channels/{channel_id}/messages?limit=30", token)
            messages = data if isinstance(data, list) else []

            msg_log = self.query_one("#msg-log", RichLog)
            self.call_from_thread(msg_log.clear)

            ch_name = self._get_channel_name(channel_id)
            self.call_from_thread(
                msg_log.write,
                f"[bold cyan]═══ #{ch_name} — {len(messages)} messages ═══[/]\n",
            )

            # Messages are newest-first, reverse for chronological order
            for msg in reversed(messages):
                author = msg.get("author", {}).get("username", "unknown")
                content = msg.get("content", "")
                ts = msg.get("timestamp", "")
                # Extract HH:MM from ISO timestamp
                time_str = ""
                if "T" in ts:
                    time_str = ts.split("T")[1][:5] if len(ts.split("T")) > 1 else ""

                # Highlight our bot
                if msg.get("author", {}).get("bot"):
                    author_style = f"[bold magenta]{author}[/]"
                else:
                    author_style = f"[bold green]{author}[/]"

                self.call_from_thread(
                    msg_log.write,
                    f"[dim]{time_str}[/] {author_style}: {content}",
                )

            self.call_from_thread(setattr, self.query_one("#status-badge", StatusBadge), "status", "idle")
        except Exception as e:
            self.call_from_thread(self.log_write, f"[red]✗[/] Fetch messages: {e}")
            self.call_from_thread(setattr, self.query_one("#status-badge", StatusBadge), "status", "error")

    @work(thread=True)
    def send_discord_message(self, channel_id: int, content: str) -> None:
        token = os.environ.get(TOKEN_ENV_VAR, "")
        if not token:
            return

        self.call_from_thread(setattr, self.query_one("#status-badge", StatusBadge), "status", "api")

        try:
            result = discord_api("POST", f"/channels/{channel_id}/messages", token, {"content": content})

            ch_name = self._get_channel_name(channel_id)
            msg_log = self.query_one("#msg-log", RichLog)
            self.call_from_thread(
                msg_log.write,
                f"[bold green]you[/] → #{ch_name}: {content}",
            )
            self.call_from_thread(
                self.log_write,
                f"[green]✓[/] Message sent to #{ch_name}",
            )
            self.call_from_thread(setattr, self.query_one("#status-badge", StatusBadge), "status", "idle")
        except Exception as e:
            self.call_from_thread(self.log_write, f"[red]✗[/] Send failed: {e}")
            self.call_from_thread(setattr, self.query_one("#status-badge", StatusBadge), "status", "error")

    @work(thread=True)
    def api_call(self, label: str, path: str) -> None:
        token = os.environ.get(TOKEN_ENV_VAR, "")
        if not token:
            return

        self.call_from_thread(setattr, self.query_one("#status-badge", StatusBadge), "status", "api")

        try:
            data = discord_api("GET", path, token)
            pretty = json.dumps(data, indent=2) if isinstance(data, (dict, list)) else str(data)

            self.call_from_thread(self.log_write, f"\n[bold yellow]━━━ {label} ━━━[/]")
            for line in pretty.split("\n"):
                self.call_from_thread(self.log_write, line)
            self.call_from_thread(setattr, self.query_one("#status-badge", StatusBadge), "status", "idle")
        except Exception as e:
            self.call_from_thread(self.log_write, f"[red]✗[/] {label}: {e}")
            self.call_from_thread(setattr, self.query_one("#status-badge", StatusBadge), "status", "error")

    # ── Helpers ──────────────────────────────────────────────────────────

    def log_write(self, text: str) -> None:
        self.query_one("#log", RichLog).write(text)

    def _get_channel_name(self, channel_id: int) -> str:
        for channels in self.channels.values():
            for ch in channels:
                if ch["id"] == channel_id:
                    return ch["name"]
        return str(channel_id)

    @work(exclusive=True, thread=True)
    def run_command(
        self,
        cmd: str,
        cwd: str | None = None,
        env: dict | None = None,
        label: str = "Command",
        timeout: int = 60,
    ) -> None:
        """Run a shell command in a background thread, streaming output to the log."""
        log = self.query_one("#log", RichLog)
        badge = self.query_one("#status-badge", StatusBadge)

        self.call_from_thread(log.write, f"\n[bold yellow]━━━ {label} ━━━[/]")
        self.call_from_thread(log.write, f"[dim]$ {cmd}[/]")
        self.call_from_thread(setattr, badge, "status", "building")

        try:
            result = subprocess.run(
                cmd,
                shell=True,
                cwd=cwd,
                env=env or os.environ,
                capture_output=True,
                text=True,
                timeout=timeout,
            )

            if result.stdout.strip():
                for line in result.stdout.strip().split("\n"):
                    self.call_from_thread(log.write, line)

            if result.stderr.strip():
                for line in result.stderr.strip().split("\n"):
                    self.call_from_thread(log.write, f"[red]{line}[/]")

            if result.returncode == 0:
                self.call_from_thread(
                    log.write, f"[green]✓ {label} completed (exit {result.returncode})[/]"
                )
                self.call_from_thread(setattr, badge, "status", "idle")
            else:
                self.call_from_thread(
                    log.write, f"[red]✗ {label} failed (exit {result.returncode})[/]"
                )
                self.call_from_thread(setattr, badge, "status", "error")

        except subprocess.TimeoutExpired:
            self.call_from_thread(log.write, f"[red]✗ {label} timed out after {timeout}s[/]")
            self.call_from_thread(setattr, badge, "status", "error")
        except Exception as e:
            self.call_from_thread(log.write, f"[red]✗ {label} error: {e}[/]")
            self.call_from_thread(setattr, badge, "status", "error")

    def action_clear_log(self) -> None:
        log = self.query_one("#log", RichLog)
        log.clear()
        log.write("[dim]Log cleared.[/]")


if __name__ == "__main__":
    app = ControlPanel()
    app.run()

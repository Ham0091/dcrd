#!/usr/bin/env python3
"""
dcrd Control Panel — A clickable Textual TUI for managing the dcrd Discord client.

Usage:
    python control.py

Buttons for: Kill Process, Rebuild, Launch Bot, Git Status/Push/Pull/Commit,
             plus a live log viewer and input field for custom commands.
"""

import asyncio
import os
import subprocess
import sys
from pathlib import Path

from textual import on, work
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical
from textual.reactive import reactive
from textual.widgets import (
    Button,
    Footer,
    Header,
    Input,
    Label,
    ProgressBar,
    RichLog,
    Static,
)

# ── Configuration ────────────────────────────────────────────────────────────
PROJECT_DIR = Path(__file__).resolve().parent
BINARY = PROJECT_DIR / "target" / "release" / "dcrd.exe"
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
        }
        return icons.get(self.status, self.status)


class ControlPanel(App):
    """dcrd Control Panel — clickable terminal GUI."""

    CSS = """
    Screen {
        layout: vertical;
    }

    #main-area {
        height: 1fr;
    }

    #sidebar {
        width: 32;
        min-width: 28;
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

    #log-area {
        height: 1fr;
        padding: 0 1;
    }

    #log {
        height: 1fr;
        border: round $primary;
        background: $surface;
    }

    #input-bar {
        height: 3;
        padding: 0 1;
    }

    #cmd-input {
        height: 3;
    }

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

    .btn-danger {
        background: $error;
        color: $text;
    }

    .btn-success {
        background: $success;
        color: $text;
    }

    .btn-warning {
        background: $warning;
        color: $text;
    }
    """

    BINDINGS = [
        Binding("q", "quit", "Quit"),
        Binding("ctrl+l", "clear_log", "Clear Log"),
    ]

    status_text: reactive[str] = reactive("idle")
    bot_process = None

    def compose(self) -> ComposeResult:
        yield Header(show_clock=True)

        with Horizontal(id="main-area"):
            # ── Sidebar with buttons ──
            with Vertical(id="sidebar"):
                yield Label("⚙  Process")
                yield Button("🛑 Kill Process", id="btn-kill", variant="error")
                yield Button("🔨 Rebuild", id="btn-rebuild", variant="warning")
                yield Button("🚀 Launch Bot", id="btn-launch", variant="success")

                yield Label("📁 Git")
                yield Button("📊 Git Status", id="btn-git-status")
                yield Button("📥 Git Pull", id="btn-git-pull")
                yield Button("📤 Git Push", id="btn-git-push")
                yield Button("💾 Commit & Push", id="btn-git-commit")

                yield Label("🔧 Tools")
                yield Button("📋 List Files", id="btn-ls")
                yield Button("🧹 Clear Log", id="btn-clear", variant="default")
                yield Button("❌ Quit", id="btn-quit", variant="default")

            # ── Log + Input ──
            with Vertical(id="log-area"):
                yield RichLog(id="log", highlight=True, markup=True, wrap=True)
                with Horizontal(id="input-bar"):
                    yield Input(
                        placeholder="Type a command and press Enter...",
                        id="cmd-input",
                    )

        # ── Status bar ──
        with Horizontal(id="status-bar"):
            yield StatusBadge(id="status-badge")
            yield Static("dcrd v0.1.0 · Token: not loaded", id="process-info")

    def on_mount(self) -> None:
        self.title = "dcrd Control Panel"
        self.sub_title = "Ultra-low-RAM Discord Client"
        log = self.query_one("#log", RichLog)
        log.write("[bold cyan]dcrd Control Panel[/] ready.")
        log.write("[dim]Click buttons or type commands below.[/]\n")

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
                f"Set it before launching the bot."
            )

        # Check binary
        if BINARY.exists():
            size_kb = BINARY.stat().st_size / 1024
            log.write(f"[green]✓[/] Binary found: {BINARY.name} ({size_kb:.0f} KB)")
        else:
            log.write(f"[yellow]⚠[/] Binary not found. Run [bold]Rebuild[/] first.")

    # ── Button handlers ───────────────────────────────────────────────────

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
            self.log_write("[red]✗[/] Cannot launch: DCRD_TOKEN not set in environment.")
            return

        self.log_write("[cyan]🚀 Launching dcrd in external cmd window...[/]")
        try:
            # Launch in a visible external cmd.exe window
            subprocess.Popen(
                [
                    "cmd", "/c",
                    f"start", "dcrd Discord Client",
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
            "git add -A && git commit -m \"update via control panel\" && git push origin main",
            cwd=str(PROJECT_DIR),
            label="Commit & Push",
        )

    @on(Button.Pressed, "#btn-ls")
    def handle_ls(self) -> None:
        self.run_command("dir /B", cwd=str(PROJECT_DIR), label="List Files")

    @on(Button.Pressed, "#btn-clear")
    def handle_clear(self) -> None:
        self.action_clear_log()

    @on(Button.Pressed, "#btn-quit")
    def handle_quit_btn(self) -> None:
        self.action_quit()

    @on(Input.Submitted, "#cmd-input")
    def handle_cmd_submit(self, event: Input.Submitted) -> None:
        cmd = event.value.strip()
        if not cmd:
            return
        self.query_one("#cmd-input", Input).value = ""
        self.run_command(cmd, cwd=str(PROJECT_DIR), label="Custom")

    # ── Helpers ───────────────────────────────────────────────────────────

    def log_write(self, text: str) -> None:
        self.query_one("#log", RichLog).write(text)

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

        self.call_from_thread(
            log.write, f"\n[bold yellow]━━━ {label} ━━━[/]"
        )
        self.call_from_thread(
            log.write, f"[dim]$ {cmd}[/]"
        )
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
                    log.write,
                    f"[red]✗ {label} failed (exit {result.returncode})[/]",
                )
                self.call_from_thread(setattr, badge, "status", "error")

        except subprocess.TimeoutExpired:
            self.call_from_thread(
                log.write, f"[red]✗ {label} timed out after {timeout}s[/]"
            )
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

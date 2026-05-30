/// TUI application state machine.
pub struct TuiApp {
    /// Current input mode
    pub mode: InputMode,
    /// Text input buffer
    pub input: String,
    /// Cursor position within the input buffer
    pub cursor: usize,
    /// Scroll offset for message history (0 = bottom/newest)
    pub scroll_offset: usize,
    /// Whether the application should exit
    pub should_quit: bool,
    /// Status message shown in the voice bar
    pub status_message: String,
}

/// Input mode determines how key events are handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Normal mode — arrow keys scroll, typing starts insert mode
    Normal,
    /// Insert mode — typing enters text, Enter sends, Esc returns to Normal
    Insert,
}

impl TuiApp {
    pub fn new() -> Self {
        TuiApp {
            mode: InputMode::Normal,
            input: String::new(),
            cursor: 0,
            scroll_offset: 0,
            should_quit: false,
            status_message: String::from(":help for commands"),
        }
    }

    /// Switch to Insert mode.
    pub fn enter_insert(&mut self) {
        self.mode = InputMode::Insert;
    }

    /// Switch to Normal mode and clear input.
    pub fn enter_normal(&mut self) {
        self.mode = InputMode::Normal;
        self.input.clear();
        self.cursor = 0;
    }

    /// Insert a character at the cursor position.
    pub fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Delete the character before the cursor (backspace).
    pub fn delete_char(&mut self) {
        if self.cursor > 0 {
            // Find the previous char boundary
            let prev = self.input[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.input.drain(prev..self.cursor);
            self.cursor = prev;
        }
    }

    /// Move cursor left.
    pub fn cursor_left(&mut self) {
        if self.cursor > 0 {
            let prev = self.input[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.cursor = prev;
        }
    }

    /// Move cursor right.
    pub fn cursor_right(&mut self) {
        if self.cursor < self.input.len() {
            let next = self.input[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.input.len());
            self.cursor = next;
        }
    }

    /// Scroll message history up (older messages).
    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    /// Scroll message history down (newer messages).
    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    /// Reset scroll to bottom (newest messages).
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    /// Take the current input and clear the buffer.
    pub fn take_input(&mut self) -> String {
        let input = self.input.clone();
        self.input.clear();
        self.cursor = 0;
        input
    }

    /// Set a temporary status message.
    pub fn set_status(&mut self, msg: String) {
        self.status_message = msg;
    }
}

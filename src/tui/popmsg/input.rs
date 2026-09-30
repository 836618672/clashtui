use super::dev::*;
use ratatui::{
    style::Stylize as _,
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

#[derive(Default)]
pub struct Input {
    buffer: String,
    cursor: usize,
}

impl Msg for Input {
    type Result = String;

    fn match_key_event(&mut self, kv: &Key) -> Route {
        match kv.code {
            KeyCode::Enter => return Route::Send,
            KeyCode::Esc => return Route::Drop,

            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.buffer.chars().count(),
            KeyCode::Char('a') if kv.ctrl => self.cursor = 0,
            KeyCode::Char('e') if kv.ctrl => self.cursor = self.buffer.chars().count(),
            KeyCode::Char('u') if kv.ctrl => {
                self.buffer.clear();
                self.cursor = 0;
            }
            KeyCode::Char(ch) if !kv.ctrl && !kv.alt && !kv.super_ => self.enter_char(ch),
            KeyCode::Backspace => self.delete_char(),
            KeyCode::Delete => self.delete_char_inplace(),
            KeyCode::Left => self.move_cursor_left(),
            KeyCode::Right => self.move_cursor_right(),
            _ => {}
        }
        Route::Keep
    }

    fn send(self, tx: Sender<Self::Result>) {
        let _ = tx.send(self.buffer);
    }

    fn render(&self, f: &mut Frame, area: Rect, block: Block, is_focused: bool) {
        let widget = {
            let byte_pos = self.byte_offset();
            let mut before = format!("{} ", self.buffer);
            let mut after = before.split_off(byte_pos);
            let cursor = after.remove(0);
            let prefix_width = UnicodeWidthStr::width(&self.buffer[..byte_pos]) as u16;
            Paragraph::new(Line::from_iter([
                Span::raw("> "),
                Span::raw(before),
                if is_focused {
                    Span::raw(cursor.to_string()).reversed()
                } else {
                    Span::raw(cursor.to_string())
                },
                Span::raw(after),
            ]))
            .block(block)
            .scroll((0, prefix_width.saturating_sub(area.width.saturating_sub(8))))
        };
        f.render_widget(widget, area);
    }

    fn size(&self) -> (u16, u16) {
        let width = UnicodeWidthStr::width(self.buffer.as_str()).max(10) as u16;
        (width + 2, 1)
    }
}

impl Input {
    pub fn new() -> Self {
        Default::default()
    }
    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.buffer = value.into();
        self.cursor = self.buffer.chars().count();
        self
    }
    pub fn with_title(self, title: String) -> MsgBuilder<Self> {
        MsgBuilder::new(self, title)
    }
}

impl Input {
    fn byte_offset(&self) -> usize {
        self.buffer
            .char_indices()
            .nth(self.cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.buffer.len())
    }

    fn delete_char(&mut self) {
        if self.cursor != 0 {
            self.buffer = self
                .buffer
                .char_indices()
                .enumerate()
                .filter_map(|(pos, (_, ch))| (pos != self.cursor - 1).then_some(ch))
                .collect();
            self.cursor = self.cursor.saturating_sub(1);
        }
    }
    fn delete_char_inplace(&mut self) {
        self.buffer = self
            .buffer
            .char_indices()
            .enumerate()
            .filter_map(|(pos, (_, ch))| (pos != self.cursor).then_some(ch))
            .collect();
    }
    fn enter_char(&mut self, ch: char) {
        let pos = self.byte_offset();
        self.buffer.insert(pos, ch);
        self.cursor = self.cursor.saturating_add(1);
    }
    fn move_cursor_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }
    fn move_cursor_right(&mut self) {
        self.cursor = self
            .cursor
            .saturating_add(1)
            .min(self.buffer.chars().count());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefilled_unicode_input_keeps_cursor_on_character_boundaries() {
        let mut input = Input::new().with_value("订阅abc");
        assert_eq!(input.cursor, 5);
        input.cursor = 1;
        input.delete_char_inplace();
        assert_eq!(input.buffer, "订abc");
        assert_eq!(input.cursor, 1);
        input.enter_char('新');
        assert_eq!(input.buffer, "订新abc");
        input.match_key_event(&"<C-u>".parse().unwrap());
        assert_eq!(input.buffer, "");
        assert_eq!(input.cursor, 0);
    }

    #[test]
    fn input_cjk_insert_between_chars() {
        let mut inp = Input {
            buffer: "你好".into(),
            cursor: 1,
        };
        inp.enter_char('中');
        assert_eq!(inp.buffer, "你中好");
        assert_eq!(inp.cursor, 2);
    }

    #[test]
    fn input_cjk_insert_at_beginning() {
        let mut inp = Input {
            buffer: "你好".into(),
            cursor: 0,
        };
        inp.enter_char('啊');
        assert_eq!(inp.buffer, "啊你好");
        assert_eq!(inp.cursor, 1);
    }

    #[test]
    fn input_cjk_insert_at_end() {
        let mut inp = Input {
            buffer: "你好".into(),
            cursor: 2,
        };
        inp.enter_char('啊');
        assert_eq!(inp.buffer, "你好啊");
        assert_eq!(inp.cursor, 3);
    }

    #[test]
    fn input_move_cursor_right_with_cjk() {
        let mut inp = Input {
            buffer: "你好".into(),
            cursor: 1,
        };
        inp.move_cursor_right();
        assert_eq!(inp.cursor, 2);
        inp.move_cursor_right();
        assert_eq!(inp.cursor, 2);
    }

    #[test]
    fn input_cjk_delete_char() {
        let mut inp = Input {
            buffer: "你中好".into(),
            cursor: 2,
        };
        inp.delete_char();
        assert_eq!(inp.buffer, "你好");
        assert_eq!(inp.cursor, 1);
    }
}

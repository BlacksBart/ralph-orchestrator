use tui_term::vt100::Parser;

pub struct TerminalWidget {
    parser: Parser,
}

impl Default for TerminalWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalWidget {
    pub fn new() -> Self {
        Self {
            parser: Parser::new(24, 80, 0),
        }
    }

    pub fn process(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
    }

    pub fn parser(&self) -> &Parser {
        &self.parser
    }
}

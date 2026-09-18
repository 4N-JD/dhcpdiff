#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    LBrace,
    RBrace,
    Semicolon,
    Text(String),
    Eof,
}

pub struct Lexer<'a> {
    input: &'a str,
    pos: usize,
    line: u32,
    column: u32,
    peeked: Option<Token>,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            pos: 0,
            line: 1,
            column: 1,
            peeked: None,
        }
    }

    pub fn line(&self) -> u32 {
        self.line
    }

    pub fn column(&self) -> u32 {
        self.column
    }

    pub fn peek(&mut self) -> Token {
        if let Some(tok) = self.peeked.clone() {
            return tok;
        }
        let tok = self.next_token_inner();
        self.peeked = Some(tok.clone());
        tok
    }

    pub fn next(&mut self) -> Token {
        if let Some(tok) = self.peeked.take() {
            return tok;
        }
        self.next_token_inner()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.input[self.pos..].chars().next()?;
        self.pos += ch.len_utf8();
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(ch)
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            while matches!(self.peek_char(), Some(c) if c.is_whitespace()) {
                self.bump();
            }
            if self.peek_char() == Some('#') {
                while matches!(self.peek_char(), Some(c) if c != '\n') {
                    self.bump();
                }
            } else {
                break;
            }
        }
    }

    fn read_string(&mut self, quote: char) -> String {
        let mut out = String::new();
        self.bump();
        while let Some(ch) = self.peek_char() {
            self.bump();
            if ch == quote {
                break;
            }
            if ch == '\\' {
                if let Some(esc) = self.bump() {
                    out.push(esc);
                }
            } else {
                out.push(ch);
            }
        }
        out
    }

    fn line_start_pos(&self) -> usize {
        self.input[..self.pos].rfind('\n').map(|i| i + 1).unwrap_or(0)
    }

    fn text_before_cursor(&self) -> &str {
        &self.input[self.line_start_pos()..self.pos]
    }

    fn is_inline_type_brace(&self) -> bool {
        let before = self.text_before_cursor();
        if before.contains("= {") || before.contains("={") {
            return true;
        }
        before.trim_end().ends_with('=')
    }

    fn is_structural_lbrace(&self) -> bool {
        !self.is_inline_type_brace()
    }

    fn read_balanced_braces(&mut self) -> String {
        let start = self.pos;
        let mut depth = 0usize;
        while let Some(ch) = self.peek_char() {
            match ch {
                '{' => {
                    depth += 1;
                    self.bump();
                }
                '}' => {
                    self.bump();
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                '"' | '\'' => {
                    let quote = ch;
                    self.bump();
                    while let Some(c) = self.peek_char() {
                        self.bump();
                        if c == quote {
                            break;
                        }
                        if c == '\\' {
                            self.bump();
                        }
                    }
                }
                _ => {
                    self.bump();
                }
            }
        }
        self.input[start..self.pos].to_string()
    }

    fn next_token_inner(&mut self) -> Token {
        self.skip_whitespace_and_comments();
        match self.peek_char() {
            None => Token::Eof,
            Some('{') if self.is_structural_lbrace() => {
                self.bump();
                Token::LBrace
            }
            Some('{') => {
                let s = self.read_balanced_braces();
                Token::Text(s)
            }
            Some('}') => {
                self.bump();
                Token::RBrace
            }
            Some(';') => {
                self.bump();
                Token::Semicolon
            }
            Some('"') => {
                let s = self.read_string('"');
                Token::Text(format!("\"{s}\""))
            }
            Some('\'') => {
                let s = self.read_string('\'');
                Token::Text(format!("'{s}'"))
            }
            Some('[') => {
                let start = self.pos;
                self.bump();
                let mut depth = 1;
                while let Some(ch) = self.peek_char() {
                    self.bump();
                    match ch {
                        '[' => depth += 1,
                        ']' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                Token::Text(self.input[start..self.pos].to_string())
            }
            _ => {
                let start = self.pos;
                while let Some(ch) = self.peek_char() {
                    if ['{', '}', ';', '"', '\''].contains(&ch) {
                        break;
                    }
                    self.bump();
                }
                let text = self.input[start..self.pos].trim().to_string();
                if text.is_empty() {
                    if let Some(ch) = self.peek_char() {
                        self.bump();
                        Token::Text(ch.to_string())
                    } else {
                        Token::Eof
                    }
                } else {
                    Token::Text(text)
                }
            }
        }
    }
}

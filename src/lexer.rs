#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Whitespace,
    Newline,
    Comment,
    Ident,
    Value,
    Block,
    Entity,
    FuncName,
    Number,
    String,
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Eq,
    Dot,
    Arrow,
    At,
    Star,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub start: usize,
    pub end: usize,
}

impl Token {
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start..self.end]
    }
}

const ENTITY_PREFIXES: [&str; 8] = ["sig", "fn", "ss", "dss", "gv", "const", "jt", "dt"];

fn numbered(word: &str, prefix: &str) -> bool {
    word.strip_prefix(prefix)
        .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
}

fn classify_word(word: &str) -> TokenKind {
    if numbered(word, "v") {
        TokenKind::Value
    } else if numbered(word, "block") {
        TokenKind::Block
    } else if ENTITY_PREFIXES.iter().any(|p| numbered(word, p)) {
        TokenKind::Entity
    } else {
        TokenKind::Ident
    }
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn is_name_byte(b: u8) -> bool {
    is_word_byte(b) || b == b'.' || b == b'$'
}

struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl Lexer<'_> {
    fn peek(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    fn eat_while(&mut self, pred: impl Fn(u8) -> bool) {
        while self.peek(0).is_some_and(&pred) {
            self.pos += 1;
        }
    }

    fn eat_to_eol(&mut self) {
        self.eat_while(|b| b != b'\n' && b != b'\r');
    }

    fn at_special_float(&self) -> bool {
        let rest = &self.src[self.pos + 1..];
        rest.starts_with("Inf") || rest.starts_with("NaN")
    }

    fn number(&mut self) {
        if matches!(self.peek(0), Some(b'-' | b'+')) {
            self.pos += 1;
        }
        let hex = self.peek(0) == Some(b'0') && matches!(self.peek(1), Some(b'x' | b'X'));
        let exponent: [u8; 2] = if hex { *b"pP" } else { *b"eE" };
        while let Some(b) = self.peek(0) {
            if is_word_byte(b) || b == b'.' {
                self.pos += 1;
            } else if matches!(b, b'+' | b'-') && exponent.contains(&self.bytes[self.pos - 1]) {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn string(&mut self) {
        self.pos += 1;
        while let Some(b) = self.peek(0) {
            match b {
                b'"' => {
                    self.pos += 1;
                    return;
                }
                b'\\' if matches!(self.peek(1), Some(b) if b != b'\n' && b != b'\r') => self.pos += 2,
                b'\n' | b'\r' => return,
                _ => self.pos += 1,
            }
        }
    }

    fn next_kind(&mut self, b: u8) -> TokenKind {
        match b {
            b'\n' => {
                self.pos += 1;
                TokenKind::Newline
            }
            b'\r' => {
                self.pos += if self.peek(1) == Some(b'\n') { 2 } else { 1 };
                TokenKind::Newline
            }
            b';' => {
                self.eat_to_eol();
                TokenKind::Comment
            }
            b if b.is_ascii_whitespace() => {
                self.eat_while(|b| b.is_ascii_whitespace() && b != b'\n' && b != b'\r');
                TokenKind::Whitespace
            }
            b'%' => {
                self.pos += 1;
                self.eat_while(is_name_byte);
                TokenKind::FuncName
            }
            b'"' => {
                self.string();
                TokenKind::String
            }
            b'-' if self.peek(1) == Some(b'>') => {
                self.pos += 2;
                TokenKind::Arrow
            }
            b'-' | b'+' if self.peek(1).is_some_and(|n| n.is_ascii_digit()) || self.at_special_float() => {
                self.number();
                TokenKind::Number
            }
            b if b.is_ascii_digit() => {
                self.number();
                TokenKind::Number
            }
            b if b.is_ascii_alphabetic() || b == b'_' => {
                let start = self.pos;
                self.eat_while(is_word_byte);
                classify_word(&self.src[start..self.pos])
            }
            b'{' | b'}' | b'(' | b')' | b'[' | b']' | b',' | b':' | b'=' | b'.' | b'@' | b'*' => {
                self.pos += 1;
                match b {
                    b'{' => TokenKind::LBrace,
                    b'}' => TokenKind::RBrace,
                    b'(' => TokenKind::LParen,
                    b')' => TokenKind::RParen,
                    b'[' => TokenKind::LBracket,
                    b']' => TokenKind::RBracket,
                    b',' => TokenKind::Comma,
                    b':' => TokenKind::Colon,
                    b'=' => TokenKind::Eq,
                    b'.' => TokenKind::Dot,
                    b'@' => TokenKind::At,
                    _ => TokenKind::Star,
                }
            }
            _ => {
                let width = self.src[self.pos..].chars().next().map_or(1, char::len_utf8);
                self.pos += width;
                TokenKind::Unknown
            }
        }
    }
}

pub fn lex(source: &str) -> Vec<Token> {
    let mut lexer = Lexer { src: source, bytes: source.as_bytes(), pos: 0 };
    let mut tokens = Vec::new();
    while let Some(b) = lexer.peek(0) {
        let start = lexer.pos;
        let kind = lexer.next_kind(b);
        tokens.push(Token { kind, start, end: lexer.pos });
    }
    tokens
}

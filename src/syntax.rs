use crate::lexer::Token;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start..self.end]
    }
}

impl From<Token> for Span {
    fn from(token: Token) -> Span {
        Span { start: token.start, end: token.end }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxError {
    pub span: Span,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct File {
    pub items: Vec<Item>,
    pub comments: Vec<Span>,
    pub errors: Vec<SyntaxError>,
}

#[derive(Debug)]
pub enum Item {
    Directive(Directive),
    Function(Function),
}

#[derive(Debug)]
pub struct Directive {
    pub span: Span,
    pub keyword: Span,
    pub args: Vec<Token>,
}

#[derive(Debug)]
pub struct Function {
    pub span: Span,
    pub name: Option<Span>,
    pub signature: Signature,
    pub decls: Vec<Decl>,
    pub blocks: Vec<Block>,
    pub closed: bool,
}

#[derive(Debug, Default)]
pub struct Signature {
    pub params: Vec<Span>,
    pub returns: Vec<Span>,
    pub call_conv: Option<Span>,
}

#[derive(Debug)]
pub struct Decl {
    pub span: Span,
    pub name: Token,
    pub rhs: Vec<Token>,
}

#[derive(Debug)]
pub struct Block {
    pub span: Span,
    pub name: Token,
    pub params: Vec<BlockParam>,
    pub insts: Vec<Inst>,
}

#[derive(Debug)]
pub struct BlockParam {
    pub span: Span,
    pub value: Token,
    pub ty: Option<Span>,
}

#[derive(Debug)]
pub struct Inst {
    pub span: Span,
    pub results: Vec<Token>,
    pub opcode: Option<Span>,
    pub type_suffix: Vec<Span>,
    pub operands: Vec<Token>,
}

use crate::lexer::{Token, TokenKind as K, lex};
use crate::syntax::*;
use crate::tables;

const DIRECTIVE_KEYWORDS: [&str; 5] = ["test", "target", "set", "feature", "isa"];
const PARAM_ATTRS: [&str; 7] = ["uext", "sext", "inreg", "sret", "returned", "stack_limit", "exact"];

struct Line {
    tokens: Vec<Token>,
}

impl Line {
    fn span(&self) -> Span {
        span_of(&self.tokens)
    }
}

fn span_of(tokens: &[Token]) -> Span {
    Span { start: tokens[0].start, end: tokens[tokens.len() - 1].end }
}

fn split_commas(tokens: &[Token]) -> Vec<&[Token]> {
    if tokens.is_empty() {
        return Vec::new();
    }
    tokens.split(|t| t.kind == K::Comma).collect()
}

struct Parser<'a> {
    source: &'a str,
    errors: Vec<SyntaxError>,
}

impl Parser<'_> {
    fn error(&mut self, span: Span, message: impl Into<String>) {
        self.errors.push(SyntaxError { span, message: message.into() });
    }

    fn text(&self, token: Token) -> &str {
        token.text(self.source)
    }

    fn starts_function(&self, line: &[Token]) -> bool {
        line.first().is_some_and(|t| t.kind == K::Ident && self.text(*t) == "function")
    }

    fn split_lines(&mut self, comments: &mut Vec<Span>) -> Vec<Line> {
        let mut lines = Vec::new();
        let mut current = Vec::new();
        for token in lex(self.source) {
            match token.kind {
                K::Whitespace => {}
                K::Comment => comments.push(token.into()),
                K::Newline => {
                    if !current.is_empty() {
                        lines.push(Line { tokens: std::mem::take(&mut current) });
                    }
                }
                K::Unknown => {
                    let message = format!("unexpected character `{}`", self.text(token));
                    self.error(token.into(), message);
                    current.push(token);
                }
                K::LBrace if self.starts_function(&current) => {
                    current.push(token);
                    lines.push(Line { tokens: std::mem::take(&mut current) });
                }
                _ => current.push(token),
            }
        }
        if !current.is_empty() {
            lines.push(Line { tokens: current });
        }
        lines
    }

    fn check_balanced(&mut self, tokens: &[Token]) {
        let mut open: Vec<Token> = Vec::new();
        for &token in tokens {
            match token.kind {
                K::LParen | K::LBracket => open.push(token),
                K::RParen | K::RBracket => {
                    let wanted = if token.kind == K::RParen { K::LParen } else { K::LBracket };
                    if open.last().is_some_and(|o| o.kind == wanted) {
                        open.pop();
                    } else {
                        let message = format!("unmatched `{}`", self.text(token));
                        self.error(token.into(), message);
                    }
                }
                _ => {}
            }
        }
        for token in open {
            let message = format!("unclosed `{}`", self.text(token));
            self.error(token.into(), message);
        }
    }

    fn type_group(&mut self, group: &[Token], signature: &mut Signature, what: &str) -> Option<Span> {
        let Some(&first) = group.first() else {
            let at = Span { start: self.source.len(), end: self.source.len() };
            self.error(at, format!("expected {what} type"));
            return None;
        };
        if first.kind != K::Ident {
            self.error(first.into(), format!("expected {what} type"));
        }
        let mut kept = 1;
        for (i, &token) in group.iter().enumerate().skip(1) {
            if token.kind != K::Ident {
                self.error(token.into(), format!("unexpected `{}` in {what}", self.text(token)));
                kept = i + 1;
            } else if PARAM_ATTRS.contains(&self.text(token)) {
                kept = i + 1;
            } else {
                signature.call_conv = Some(token.into());
            }
        }
        Some(span_of(&group[..kept]))
    }

    fn function_header(&mut self, line: &Line) -> Function {
        let t = &line.tokens;
        let mut function = Function {
            span: line.span(),
            name: None,
            signature: Signature::default(),
            decls: Vec::new(),
            blocks: Vec::new(),
            closed: false,
        };
        let mut i = 1;
        match t.get(i) {
            Some(token) if token.kind == K::FuncName => {
                function.name = Some((*token).into());
                i += 1;
            }
            Some(token)
                if token.kind == K::Ident
                    && t.get(i + 1).is_some_and(|c| c.kind == K::Colon)
                    && t.get(i + 2).is_some_and(|n| n.kind == K::Number) =>
            {
                function.name = Some(span_of(&t[i..i + 3]));
                i += 3;
            }
            other => {
                let at = other.map_or(Span { start: line.span().end, end: line.span().end }, |t| (*t).into());
                self.error(at, "expected function name");
            }
        }

        let opens_body = t[t.len() - 1].kind == K::LBrace;
        let end = if opens_body { t.len() - 1 } else { t.len() };

        if t.get(i).is_some_and(|p| p.kind == K::LParen) {
            let close = t[i..end].iter().position(|p| p.kind == K::RParen).map(|p| p + i);
            let inner_end = close.unwrap_or(end);
            let mut signature = Signature::default();
            let groups = split_commas(&t[i + 1..inner_end]);
            for group in groups {
                if let Some(span) = self.type_group(group, &mut signature, "parameter") {
                    signature.params.push(span);
                }
            }
            match close {
                Some(close) => i = close + 1,
                None => {
                    self.error(t[i].into(), "unclosed `(`");
                    i = end;
                }
            }
            if t.get(i).is_some_and(|a| a.kind == K::Arrow) && i < end {
                i += 1;
                let groups = split_commas(&t[i..end]);
                if groups.is_empty() {
                    self.error(t[i - 1].into(), "expected return type after `->`");
                }
                for group in groups {
                    if let Some(span) = self.type_group(group, &mut signature, "return") {
                        signature.returns.push(span);
                    }
                }
            } else {
                for &token in &t[i..end] {
                    if token.kind == K::Ident {
                        signature.call_conv = Some(token.into());
                    } else {
                        self.error(token.into(), format!("unexpected `{}` in function header", self.text(token)));
                    }
                }
            }
            function.signature = signature;
        } else {
            let at = t.get(i).map_or(Span { start: line.span().end, end: line.span().end }, |t| (*t).into());
            self.error(at, "expected `(` after function name");
        }

        if !opens_body {
            self.error(Span { start: line.span().end, end: line.span().end }, "expected `{`");
        }
        function
    }

    fn declaration(&mut self, line: &Line) -> Decl {
        let t = &line.tokens;
        if t.get(1).is_none_or(|eq| eq.kind != K::Eq) {
            self.error(t[0].into(), "expected `=` after declaration name");
        }
        let rhs: Vec<Token> = t.iter().skip(2).copied().collect();
        self.check_balanced(&rhs);
        Decl { span: line.span(), name: t[0], rhs }
    }

    fn block_header(&mut self, line: &Line) -> Block {
        let t = &line.tokens;
        let mut block = Block { span: line.span(), name: t[0], params: Vec::new(), insts: Vec::new() };
        let mut i = 1;
        if t.get(i).is_some_and(|p| p.kind == K::LParen) {
            let close = t.iter().position(|p| p.kind == K::RParen);
            let inner_end = close.unwrap_or(t.len());
            for group in split_commas(&t[i + 1..inner_end]) {
                match group {
                    [value, colon, ty @ ..] if value.kind == K::Value && colon.kind == K::Colon && !ty.is_empty() => {
                        block.params.push(BlockParam {
                            span: span_of(group),
                            value: *value,
                            ty: Some(span_of(ty)),
                        });
                    }
                    [] => self.error(t[i].into(), "expected block parameter"),
                    _ => self.error(span_of(group), "expected `vN: type`"),
                }
            }
            match close {
                Some(close) => i = close + 1,
                None => {
                    self.error(t[i].into(), "unclosed `(`");
                    i = t.len();
                }
            }
        }
        while t.get(i).is_some_and(|a| a.kind == K::Ident) {
            i += 1;
        }
        if t.get(i).is_some_and(|c| c.kind == K::Colon) {
            i += 1;
        } else {
            self.error(Span { start: line.span().end, end: line.span().end }, "expected `:` after block header");
        }
        if i < t.len() {
            self.error(span_of(&t[i..]), "unexpected tokens after block header");
        }
        block
    }

    fn instruction(&mut self, line: &Line) -> Inst {
        let t = &line.tokens;
        let mut inst = Inst {
            span: line.span(),
            results: Vec::new(),
            opcode: None,
            type_suffix: Vec::new(),
            operands: Vec::new(),
        };
        let mut i = 0;
        while t.get(i).is_some_and(|a| a.kind == K::At) {
            i += 1;
            if t.get(i).is_some_and(|n| n.kind == K::Number) {
                i += 1;
            }
        }

        while t.get(i).is_some_and(|v| v.kind == K::Value) {
            inst.results.push(t[i]);
            i += 1;
            if t.get(i).is_some_and(|c| c.kind == K::Comma) {
                i += 1;
                if t.get(i).is_none_or(|v| v.kind != K::Value) {
                    self.error(t[i - 1].into(), "expected result value after `,`");
                }
            } else {
                break;
            }
        }
        if !inst.results.is_empty() {
            if t.get(i).is_some_and(|e| e.kind == K::Eq) {
                i += 1;
            } else {
                let at = t.get(i).map_or(Span { start: line.span().end, end: line.span().end }, |t| (*t).into());
                self.error(at, "expected `=` after results");
            }
        }

        match t.get(i) {
            Some(op) if op.kind == K::Ident => {
                let name = self.text(*op);
                if tables::opcode(name).is_none() {
                    let message = format!("unknown opcode `{name}`");
                    self.error((*op).into(), message);
                }
                inst.opcode = Some((*op).into());
                i += 1;
                while t.get(i).is_some_and(|d| d.kind == K::Dot) {
                    match t.get(i + 1) {
                        Some(ty) if ty.kind == K::Ident => {
                            inst.type_suffix.push((*ty).into());
                            i += 2;
                        }
                        _ => {
                            self.error(t[i].into(), "expected type after `.`");
                            i += 1;
                        }
                    }
                }
            }
            Some(other) if other.kind != K::Unknown => {
                let message = format!("expected opcode, found `{}`", self.text(*other));
                self.error((*other).into(), message);
            }
            Some(_) => {}
            None => self.error(Span { start: line.span().end, end: line.span().end }, "expected opcode"),
        }

        inst.operands = t[i.min(t.len())..].to_vec();
        if let Some(last) = inst.operands.last().filter(|l| l.kind == K::Comma) {
            self.error((*last).into(), "expected operand after `,`");
        }
        self.check_balanced(&inst.operands);
        inst
    }

    fn body_line(&mut self, function: &mut Function, line: &Line) -> bool {
        let t = &line.tokens;
        function.span.end = line.span().end;
        match t[0].kind {
            K::RBrace => {
                if t.len() > 1 {
                    self.error(span_of(&t[1..]), "unexpected tokens after `}`");
                }
                function.closed = true;
                return true;
            }
            K::Entity => {
                let decl = self.declaration(line);
                function.decls.push(decl);
            }
            K::Block => {
                let block = self.block_header(line);
                function.blocks.push(block);
            }
            _ => {
                let inst = self.instruction(line);
                match function.blocks.last_mut() {
                    Some(block) => block.insts.push(inst),
                    None => self.error(line.span(), "instruction outside a block"),
                }
            }
        }
        false
    }

    fn unclosed(&mut self, function: &Function) {
        let start = function.span.start;
        self.error(Span { start, end: start + "function".len() }, "unclosed function body");
    }
}

pub fn parse(source: &str) -> File {
    let mut file = File::default();
    let mut parser = Parser { source, errors: Vec::new() };
    let lines = parser.split_lines(&mut file.comments);
    let mut current: Option<Function> = None;

    for line in &lines {
        let first = line.tokens[0];
        let is_function_kw = first.kind == K::Ident && parser.text(first) == "function";

        if let Some(mut function) = current.take() {
            if !is_function_kw {
                if !parser.body_line(&mut function, line) {
                    current = Some(function);
                } else {
                    file.items.push(Item::Function(function));
                }
                continue;
            }
            parser.unclosed(&function);
            file.items.push(Item::Function(function));
        }

        if is_function_kw {
            current = Some(parser.function_header(line));
        } else if first.kind == K::Ident && DIRECTIVE_KEYWORDS.contains(&parser.text(first)) {
            file.items.push(Item::Directive(Directive {
                span: line.span(),
                keyword: first.into(),
                args: line.tokens[1..].to_vec(),
            }));
        } else {
            let message = format!("unexpected `{}` outside a function", parser.text(first));
            parser.error(line.span(), message);
        }
    }

    if let Some(function) = current {
        parser.unclosed(&function);
        file.items.push(Item::Function(function));
    }

    parser.errors.sort_by_key(|e| (e.span.start, e.span.end));
    file.errors = parser.errors;
    file
}

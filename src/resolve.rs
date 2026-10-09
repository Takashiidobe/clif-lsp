use std::collections::HashMap;

use crate::lexer::{Token, TokenKind};
use crate::syntax::{File, Item, Span, SyntaxError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NameKind {
    Value,
    Block,
    Entity,
    Function,
}

impl NameKind {
    pub fn label(self) -> &'static str {
        match self {
            NameKind::Value => "value",
            NameKind::Block => "block",
            NameKind::Entity => "entity",
            NameKind::Function => "function",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Def {
    pub kind: NameKind,
    pub name: String,
    pub span: Span,
    pub scope: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct Use {
    pub kind: NameKind,
    pub name: String,
    pub span: Span,
    pub def: Option<usize>,
}

#[derive(Debug, Default)]
pub struct Resolution {
    pub defs: Vec<Def>,
    pub uses: Vec<Use>,
    pub problems: Vec<SyntaxError>,
}

fn kind_of(token: Token) -> Option<NameKind> {
    match token.kind {
        TokenKind::Value => Some(NameKind::Value),
        TokenKind::Block => Some(NameKind::Block),
        TokenKind::Entity => Some(NameKind::Entity),
        TokenKind::FuncName => Some(NameKind::Function),
        _ => None,
    }
}

struct Resolver<'a> {
    source: &'a str,
    out: Resolution,
    functions: HashMap<String, usize>,
    scope: HashMap<(NameKind, String), usize>,
    scope_id: usize,
    pending: Vec<(NameKind, String, Span)>,
}

impl Resolver<'_> {
    fn define(&mut self, kind: NameKind, span: Span) {
        let name = span.text(self.source).to_owned();
        let id = self.out.defs.len();
        self.out.defs.push(Def { kind, name: name.clone(), span, scope: Some(self.scope_id) });
        if self.scope.contains_key(&(kind, name.clone())) {
            self.out.problems.push(SyntaxError {
                span,
                message: format!("{} `{name}` is defined more than once", kind.label()),
            });
        } else {
            self.scope.insert((kind, name), id);
        }
    }

    fn reference(&mut self, tokens: &[Token]) {
        for &token in tokens {
            if let Some(kind) = kind_of(token) {
                let span = Span::from(token);
                self.pending.push((kind, span.text(self.source).to_owned(), span));
            }
        }
    }

    fn finish_function(&mut self) {
        for (kind, name, span) in std::mem::take(&mut self.pending) {
            let def = match kind {
                NameKind::Function => self.functions.get(&name).copied(),
                _ => self.scope.get(&(kind, name.clone())).copied(),
            };
            if def.is_none() && kind != NameKind::Function {
                self.out.problems.push(SyntaxError {
                    span,
                    message: format!("undefined {} `{name}`", kind.label()),
                });
            }
            self.out.uses.push(Use { kind, name, span, def });
        }
        self.scope.clear();
    }
}

pub fn resolve(file: &File, source: &str) -> Resolution {
    let mut resolver = Resolver {
        source,
        out: Resolution::default(),
        functions: HashMap::new(),
        scope: HashMap::new(),
        scope_id: 0,
        pending: Vec::new(),
    };

    for item in &file.items {
        let Item::Function(function) = item else { continue };
        let Some(name) = function.name.filter(|n| n.text(source).starts_with('%')) else { continue };
        let text = name.text(source).to_owned();
        let id = resolver.out.defs.len();
        resolver.out.defs.push(Def { kind: NameKind::Function, name: text.clone(), span: name, scope: None });
        if resolver.functions.contains_key(&text) {
            resolver.out.problems.push(SyntaxError {
                span: name,
                message: format!("function `{text}` is defined more than once"),
            });
        } else {
            resolver.functions.insert(text, id);
        }
    }

    for (index, item) in file.items.iter().enumerate() {
        let Item::Function(function) = item else { continue };
        resolver.scope_id = index;
        for decl in &function.decls {
            resolver.define(NameKind::Entity, decl.name.into());
            resolver.reference(&decl.rhs);
        }
        for block in &function.blocks {
            resolver.define(NameKind::Block, block.name.into());
            for param in &block.params {
                resolver.define(NameKind::Value, param.value.into());
            }
            for inst in &block.insts {
                for &result in &inst.results {
                    resolver.define(NameKind::Value, result.into());
                }
                resolver.reference(&inst.operands);
            }
        }
        resolver.finish_function();
    }

    resolver.out.problems.sort_by_key(|p| (p.span.start, p.span.end));
    resolver.out
}

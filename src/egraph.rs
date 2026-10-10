use std::collections::HashMap;

use crate::egraph_trace::{self, Entry};
use crate::html::{self, escape};
use crate::stages::Target;

#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub file: String,
    pub line: u32,
}

#[derive(Clone, Debug)]
pub struct Member {
    pub text: String,
    pub cost: Option<u32>,
    pub original: bool,
    pub chosen: bool,
    pub rules: Vec<Rule>,
}

#[derive(Clone, Debug)]
pub struct Class {
    pub value: String,
    pub input: String,
    pub members: Vec<Member>,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub name: String,
    pub classes: Vec<Class>,
    pub stats: Vec<(String, u64)>,
}

#[derive(Debug)]
enum Def {
    Param,
    Result(String),
    Union(String, String),
}

#[derive(Default)]
struct Parsed {
    inputs: Vec<(String, String)>,
    mapping: Vec<(String, String)>,
    defs: HashMap<String, Def>,
    insts: HashMap<String, String>,
    costs: HashMap<String, Option<u32>>,
    best: HashMap<String, String>,
    results: HashMap<String, (Vec<String>, Vec<Rule>)>,
    stats: Vec<(String, u64)>,
}

pub fn analyze(target: &Target) -> Result<Report, String> {
    let entries = egraph_trace::capture(target)?;
    Ok(Report { name: target.name.clone(), ..from_entries(&entries) })
}

pub fn from_entries(entries: &[Entry]) -> Report {
    let parsed = parse(entries);
    let mut classes = Vec::new();
    for (value, root) in &parsed.mapping {
        let Some((_, input)) = parsed.inputs.iter().find(|(v, _)| v == value) else { continue };
        let mut leaves = Vec::new();
        parsed.leaves(root, &mut leaves);
        let chosen = parsed.best.get(root).cloned().unwrap_or_else(|| root.clone());
        let produced = parsed.results.get(value);
        let members = leaves
            .iter()
            .map(|leaf| Member {
                text: parsed.describe(leaf),
                cost: parsed.costs.get(leaf).copied().flatten(),
                original: leaf == value,
                chosen: *leaf == chosen,
                rules: produced.filter(|(made, _)| made.contains(leaf)).map(|(_, rules)| rules.clone()).unwrap_or_default(),
            })
            .collect();
        classes.push(Class { value: value.clone(), input: input.clone(), members });
    }
    Report { name: String::new(), classes, stats: parsed.stats }
}

impl Parsed {
    fn leaves(&self, value: &str, out: &mut Vec<String>) {
        match self.defs.get(value) {
            Some(Def::Union(a, b)) => {
                self.leaves(a, out);
                self.leaves(b, out);
            }
            _ => {
                if !out.iter().any(|v| v == value) {
                    out.push(value.to_owned());
                }
            }
        }
    }

    fn describe(&self, value: &str) -> String {
        match self.defs.get(value) {
            Some(Def::Result(inst)) => self.insts.get(inst).map(|data| self.render(data)).unwrap_or_else(|| value.to_owned()),
            _ => value.to_owned(),
        }
    }

    fn constant(&self, value: &str) -> Option<String> {
        let Some(Def::Result(inst)) = self.defs.get(value) else { return None };
        let data = self.insts.get(inst)?;
        let fields = fields_of(data)?;
        (fields.iter().any(|(k, v)| k == "opcode" && v == "Iconst")).then(|| fields.iter().find(|(k, _)| k == "imm").map(|(_, v)| unwrap_call(v))).flatten()
    }

    fn operand(&self, value: &str) -> String {
        if let Some(constant) = self.constant(value) {
            return constant;
        }
        let input = self.mapping.iter().find(|(_, root)| root == value).map(|(orig, _)| orig);
        input.map_or_else(|| value.to_owned(), Clone::clone)
    }

    fn render(&self, data: &str) -> String {
        let Some(fields) = fields_of(data) else { return data.to_owned() };
        let mut operands = Vec::new();
        let mut opcode = String::new();
        for (key, value) in &fields {
            match key.as_str() {
                "opcode" => opcode = value.to_lowercase(),
                "args" => operands.extend(list_of(value).iter().map(|v| self.operand(v))),
                "arg" => operands.push(self.operand(value)),
                "imm" => operands.push(unwrap_call(value)),
                _ => operands.push(value.to_lowercase()),
            }
        }
        if operands.is_empty() { opcode } else { format!("{opcode} {}", operands.join(", ")) }
    }
}

fn parse(entries: &[Entry]) -> Parsed {
    let mut parsed = Parsed::default();
    let mut processing: HashMap<String, String> = HashMap::new();
    let mut frames: Vec<(String, Vec<Rule>)> = Vec::new();
    let mut computing = String::new();
    for entry in entries {
        let message = entry.message.trim_start();
        let target = entry.target.as_str();
        if target == "cranelift_codegen::opts::generated_code" {
            if let Some(rule) = rule_of(message) {
                if let Some((_, rules)) = frames.last_mut() {
                    rules.push(rule);
                }
            }
        } else if target == "cranelift_codegen::egraph::elaborate" {
            if let Some(rest) = message.strip_prefix("computing best for value ") {
                computing = rest.split_whitespace().next().unwrap_or("").to_owned();
            } else if let Some(rest) = message.strip_prefix("-> cost of value ") {
                if let Some((value, cost)) = rest.split_once(" = ") {
                    parsed.costs.insert(value.to_owned(), cost_of(cost));
                }
            } else if let Some(rest) = message.strip_prefix("-> best of union(") {
                if let Some((_, tail)) = rest.rsplit_once("= BestEntry(") {
                    if let Some(leaf) = tail.trim_end_matches(')').rsplit(", ").next() {
                        parsed.best.insert(computing.clone(), leaf.to_owned());
                    }
                }
            }
        } else if target == "cranelift_codegen::egraph" {
            if let Some(rest) = message.strip_prefix("Processing inst ") {
                if let Some((inst, text)) = rest.split_once(": ") {
                    processing.insert(inst.to_owned(), text.split("  ;").next().unwrap_or(text).to_owned());
                }
            } else if let Some(rest) = message.strip_prefix("optimizing inst ") {
                let mut words = rest.split_whitespace();
                let (inst, orig, gave) = (words.next(), words.nth(2), words.nth(1));
                if let (Some(inst), Some(orig), Some(gave)) = (inst, orig, gave) {
                    if let Some(text) = processing.get(inst) {
                        parsed.inputs.push((orig.to_owned(), text.clone()));
                        parsed.mapping.push((orig.to_owned(), gave.to_owned()));
                    }
                }
            } else if let Some(rest) = message.strip_prefix("Calling into ISLE with original value ") {
                frames.push((rest.trim().to_owned(), Vec::new()));
            } else if let Some(rest) = message.strip_prefix("-> returned from ISLE: ") {
                if let (Some((value, list)), Some((_, rules))) = (rest.split_once(" -> "), frames.pop()) {
                    parsed.results.insert(value.to_owned(), (list_of(list), rules));
                }
            } else if let Some(rest) = message.strip_prefix("-> ") {
                dump_line(&mut parsed, rest);
            } else if let Some(rest) = message.strip_prefix("stats: Stats {") {
                for line in rest.lines() {
                    if let Some((key, count)) = line.trim().trim_end_matches(',').split_once(": ") {
                        if let Ok(count) = count.parse() {
                            parsed.stats.push((key.to_owned(), count));
                        }
                    }
                }
            }
        }
    }
    parsed
}

fn dump_line(parsed: &mut Parsed, line: &str) {
    let Some((name, definition)) = line.split_once(" = ") else { return };
    if name.starts_with("inst") && name[4..].chars().all(|c| c.is_ascii_digit()) {
        parsed.insts.insert(name.to_owned(), definition.to_owned());
    } else if name.starts_with('v') && name[1..].chars().all(|c| c.is_ascii_digit()) {
        if let Some(inner) = definition.strip_prefix("Result(").and_then(|d| d.strip_suffix(')')) {
            if let Some((inst, _)) = inner.split_once(", ") {
                parsed.defs.insert(name.to_owned(), Def::Result(inst.to_owned()));
            }
        } else if let Some(inner) = definition.strip_prefix("Union(").and_then(|d| d.strip_suffix(')')) {
            if let Some((a, b)) = inner.split_once(", ") {
                parsed.defs.insert(name.to_owned(), Def::Union(a.to_owned(), b.to_owned()));
            }
        } else if definition.starts_with("Param(") {
            parsed.defs.insert(name.to_owned(), Def::Param);
        }
    }
}

fn rule_of(message: &str) -> Option<Rule> {
    let rest = message.strip_prefix("ISLE simplify ")?;
    let (file, line) = rest.split_once(" line ")?;
    Some(Rule { file: file.to_owned(), line: line.trim().parse().ok()? })
}

fn cost_of(text: &str) -> Option<u32> {
    text.strip_prefix("Cost::Finite(")?.strip_suffix(')')?.parse().ok()
}

fn list_of(text: &str) -> Vec<String> {
    let inner = text.trim().trim_start_matches('[').trim_end_matches(']');
    inner.split(',').map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned).collect()
}

fn unwrap_call(text: &str) -> String {
    match (text.find('('), text.strip_suffix(')')) {
        (Some(open), Some(stripped)) => stripped[open + 1..].to_owned(),
        _ => text.to_owned(),
    }
}

fn fields_of(data: &str) -> Option<Vec<(String, String)>> {
    let open = data.find('{')?;
    let body = data[open + 1..].trim().strip_suffix('}')?;
    let mut fields = Vec::new();
    let (mut depth, mut start) = (0usize, 0usize);
    for (index, c) in body.char_indices() {
        match c {
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                fields.push(&body[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    fields.push(&body[start..]);
    fields.into_iter().map(|f| f.trim()).filter(|f| !f.is_empty()).map(|f| f.split_once(": ").map(|(k, v)| (k.to_owned(), v.to_owned()))).collect()
}

const CSS: &str = r#"
.card { border: 1px solid var(--border); border-radius: 6px; margin: 0 0 12px; max-width: 900px; overflow: hidden; }
h2 { font-size: 14px; margin: 16px 0 8px; color: var(--muted); }
.card .head { background: var(--head); padding: 4px 12px; font-weight: 600; }
.form { display: grid; grid-template-columns: minmax(180px, 1fr) 160px minmax(200px, 1fr); align-items: center; padding: 0 12px; border-top: 1px solid var(--border); }
.form.chosen { background: var(--new); }
.bar { display: block; height: 10px; background: var(--line); border-radius: 2px; }
.cost { color: var(--muted); }
.rule { color: var(--muted); font-size: 12px; overflow-wrap: anywhere; }
.empty { color: var(--muted); }
"#;

fn class_card(body: &mut String, class: &Class) {
    let has_choice = class.members.len() > 1;
    body.push_str(&format!("<div class=\"card\">\n<div class=\"head mono\">{}</div>\n", escape(&class.input)));
    let worst = class.members.iter().filter_map(|m| m.cost).max().unwrap_or(1).max(1);
    for member in &class.members {
        let chosen = if has_choice && member.chosen { " chosen" } else { "" };
        let width = member.cost.map_or(0, |cost| cost * 140 / worst);
        let cost = member.cost.map_or("cost unknown".to_owned(), |cost| format!("cost {cost}"));
        let rules: Vec<String> = member.rules.iter().map(|r| escape(&format!("{}:{}", r.file, r.line))).collect();
        body.push_str(&format!(
            "<div class=\"form mono{chosen}\"><span>{}</span><span class=\"cost\"><i class=\"bar\" style=\"width:{width}px\"></i>{cost}</span><span class=\"rule\">{}</span></div>\n",
            escape(&member.text),
            rules.join(", ")
        ));
    }
    body.push_str("</div>\n");
}

impl Report {
    pub fn with_alternatives(&self) -> impl Iterator<Item = &Class> {
        self.classes.iter().filter(|class| class.members.len() > 1)
    }

    pub fn rewritten(&self) -> usize {
        self.classes.iter().filter(|class| class.members.iter().any(|m| m.chosen && !m.original)).count()
    }

    pub fn html(&self) -> String {
        let mut body = String::new();
        if self.classes.is_empty() {
            body.push_str("<p class=\"empty\">No egraph data. The egraph pass runs only with opt_level=speed or speed_and_size.</p>\n");
        }
        let (multiple, single): (Vec<&Class>, Vec<&Class>) = self.classes.iter().partition(|class| class.members.len() > 1);
        for (heading, group) in [("Rewritten or with alternatives", multiple), ("No alternatives", single)] {
            if group.is_empty() {
                continue;
            }
            body.push_str(&format!("<h2>{heading}</h2>\n"));
            for class in group {
                class_card(&mut body, class);
            }
        }
        html::page(&format!("E-graph of {}", self.name), &[("new", "chosen by the extractor")], CSS, &body, "")
    }

    pub fn text(&self) -> String {
        let mut out = String::new();
        for class in &self.classes {
            out.push_str(&format!("class {}\n", class.input));
            for member in &class.members {
                let mut line = format!("  {}", member.text);
                match member.cost {
                    Some(cost) => line.push_str(&format!(" | cost {cost}")),
                    None => line.push_str(" | cost unknown"),
                }
                if member.original {
                    line.push_str(" | original");
                }
                if member.chosen {
                    line.push_str(" | chosen");
                }
                for rule in &member.rules {
                    line.push_str(&format!(" | {}:{}", rule.file, rule.line));
                }
                out.push_str(&line);
                out.push('\n');
            }
        }
        out
    }
}

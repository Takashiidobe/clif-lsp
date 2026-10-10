use similar::{ChangeTag, TextDiff};

use crate::html::{self, escape};
use crate::stages::Snapshot;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Same,
    Added,
    Removed,
    Changed,
}

pub struct Step<'a> {
    pub snapshot: &'a Snapshot,
    pub lines: Vec<(Mark, String)>,
    pub added: usize,
    pub removed: usize,
    pub first: bool,
}

impl Step<'_> {
    pub fn note(&self) -> String {
        if let Some(reason) = self.snapshot.skipped {
            format!("skipped: {reason}")
        } else if self.first {
            "the input".to_owned()
        } else if !self.snapshot.ir {
            "machine code for the target, not diffed against CLIF".to_owned()
        } else if self.added + self.removed == 0 {
            "no change".to_owned()
        } else {
            format!("{} added, {} removed", self.added, self.removed)
        }
    }
}

fn compare(previous: &str, current: &str) -> Vec<(Mark, String)> {
    let diff = TextDiff::from_lines(previous, current);
    let mut lines = Vec::new();
    for op in diff.ops() {
        let changes: Vec<_> = diff.iter_changes(op).collect();
        let replaced = changes.iter().any(|c| c.tag() == ChangeTag::Delete) && changes.iter().any(|c| c.tag() == ChangeTag::Insert);
        for change in changes {
            let mark = match (change.tag(), replaced) {
                (ChangeTag::Equal, _) => Mark::Same,
                (ChangeTag::Delete, _) => Mark::Removed,
                (ChangeTag::Insert, true) => Mark::Changed,
                (ChangeTag::Insert, false) => Mark::Added,
            };
            lines.push((mark, change.value().trim_end_matches('\n').to_owned()));
        }
    }
    lines
}

pub fn steps(snapshots: &[Snapshot]) -> Vec<Step<'_>> {
    snapshots
        .iter()
        .enumerate()
        .map(|(index, snapshot)| {
            let lines = match index.checked_sub(1).map(|previous| &snapshots[previous]) {
                Some(previous) if snapshot.ir => compare(&previous.text, &snapshot.text),
                _ => snapshot.text.lines().map(|line| (Mark::Same, line.to_owned())).collect(),
            };
            let added = lines.iter().filter(|(mark, _)| matches!(mark, Mark::Added | Mark::Changed)).count();
            let removed = lines.iter().filter(|(mark, _)| *mark == Mark::Removed).count();
            Step { snapshot, lines, added, removed, first: index == 0 }
        })
        .collect()
}

const CSS: &str = r#"
nav { display: flex; flex-wrap: wrap; gap: 4px 12px; margin: 0 0 16px; }
nav a, .arrows a { color: var(--fg); text-decoration: none; padding: 2px 8px; border: 1px solid var(--border); border-radius: 3px; }
nav a.cur { background: var(--hl); color: var(--bg); border-color: var(--hl); }
nav a:hover, .arrows a:hover { outline: 2px solid var(--hl); outline-offset: -2px; }
section { display: none; }
section.cur { display: block; }
h2 { font-size: 14px; margin: 0 0 4px; }
.note { color: var(--muted); margin: 0 0 12px; }
.arrows { display: flex; justify-content: space-between; margin: 0 0 12px; max-width: 900px; }
.arrows span { color: var(--muted); padding: 2px 8px; }
.code { max-width: 900px; border: 1px solid var(--border); overflow: auto; }
.line { white-space: pre; padding: 0 12px; }
.line.added { background: var(--new); }
.line.changed { background: var(--changed); }
.line.removed { background: var(--removed); text-decoration: line-through; }
"#;

const SCRIPT: &str = r#"const steps = [...document.querySelectorAll('section')];
const links = [...document.querySelectorAll('nav a')];
let current = 0;
function show() {
  const found = steps.findIndex((s) => '#' + s.id === location.hash);
  current = found < 0 ? 0 : found;
  steps.forEach((s, i) => s.classList.toggle('cur', i === current));
  links.forEach((a, i) => a.classList.toggle('cur', i === current));
}
addEventListener('hashchange', show);
addEventListener('keydown', (e) => {
  const next = e.key === 'ArrowRight' ? current + 1 : e.key === 'ArrowLeft' ? current - 1 : -1;
  if (next >= 0 && next < steps.length) location.hash = '#' + steps[next].id;
});
show();
"#;

pub fn stepper(function: &str, steps: &[Step]) -> String {
    let id = |index: usize| format!("step-{}", index + 1);
    let mut body = String::from("<nav>");
    for (index, step) in steps.iter().enumerate() {
        let class = if index == 0 { " class=\"cur\"" } else { "" };
        body.push_str(&format!("<a href=\"#{}\"{class}>{} {}</a>", id(index), index + 1, escape(step.snapshot.name)));
    }
    body.push_str("</nav>\n");
    for (index, step) in steps.iter().enumerate() {
        let class = if index == 0 { " class=\"cur\"" } else { "" };
        let previous = match index.checked_sub(1) {
            Some(previous) => format!("<a href=\"#{}\">&larr; {}</a>", id(previous), escape(steps[previous].snapshot.name)),
            None => "<span></span>".to_owned(),
        };
        let next = match steps.get(index + 1) {
            Some(next) => format!("<a href=\"#{}\">{} &rarr;</a>", id(index + 1), escape(next.snapshot.name)),
            None => "<span></span>".to_owned(),
        };
        body.push_str(&format!(
            "<section id=\"{}\"{class}>\n<h2>{} of {} &middot; {}</h2>\n<p class=\"note\">{}</p>\n<div class=\"arrows\">{previous}{next}</div>\n<div class=\"code mono\">\n",
            id(index),
            index + 1,
            steps.len(),
            escape(step.snapshot.name),
            escape(&step.note())
        ));
        for (mark, line) in &step.lines {
            let class = match mark {
                Mark::Same => "line",
                Mark::Added => "line added",
                Mark::Removed => "line removed",
                Mark::Changed => "line changed",
            };
            body.push_str(&format!("<div class=\"{class}\">{}</div>\n", escape(line)));
        }
        body.push_str("</div>\n</section>\n");
    }
    let legend = [("new", "added"), ("changed", "changed"), ("removed", "removed")];
    html::page(&format!("Optimization passes of {function}"), &legend, CSS, &body, SCRIPT)
}

use crate::html::{self, escape};
use crate::stages::{self, Tagged, Target};

pub struct Line {
    pub offset: u64,
    pub bytes: Vec<u8>,
    pub text: String,
    pub inst: Option<usize>,
}

pub struct Report {
    pub name: String,
    pub triple: String,
    pub insts: Vec<String>,
    pub lines: Vec<Line>,
    pub code_bytes: usize,
}

pub fn analyze(target: &Target) -> Result<Report, String> {
    let isa = &*target.isa;
    let tagged = stages::compile_tagged(target, |_, _| {})?;
    let spans = tagged.spans();
    let Tagged { context, insts, compiled, .. } = tagged;
    let inst_at = |offset: u64| spans.iter().find(|&&(start, end, _)| u64::from(start) <= offset && offset < u64::from(end)).map(|&(_, _, inst)| inst);

    let capstone = isa.to_capstone().map_err(|e| format!("disassembly is not available for {}: {e}", isa.triple()))?;
    let code = compiled.buffer.data();
    let decoded = capstone.disasm_all(code, 0).map_err(|e| e.to_string())?;
    let mut lines: Vec<Line> = decoded
        .iter()
        .map(|insn| {
            let operands = insn.op_str().unwrap_or("");
            let text = [insn.mnemonic().unwrap_or(""), operands].iter().filter(|part| !part.is_empty()).copied().collect::<Vec<_>>().join(" ");
            Line { offset: insn.address(), bytes: insn.bytes().to_vec(), text, inst: inst_at(insn.address()) }
        })
        .collect();

    let covered: usize = lines.iter().map(|l| l.bytes.len()).sum();
    for (index, chunk) in code[covered..].chunks(8).enumerate() {
        let offset = (covered + index * 8) as u64;
        let bytes: Vec<String> = chunk.iter().map(|b| format!("{b:#04x}")).collect();
        lines.push(Line { offset, bytes: chunk.to_vec(), text: format!(".byte {}", bytes.join(", ")), inst: inst_at(offset) });
    }

    Ok(Report {
        name: target.name.clone(),
        triple: isa.triple().to_string(),
        insts: insts.iter().map(|&inst| context.func.dfg.display_inst(inst).to_string()).collect(),
        lines,
        code_bytes: code.len(),
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

impl Report {
    pub fn clif_for_row(&self, index: usize) -> &str {
        let line = &self.lines[index];
        let starts_run = index == 0 || self.lines[index - 1].inst != line.inst;
        match line.inst {
            Some(inst) if starts_run => &self.insts[inst],
            _ => "",
        }
    }

    pub fn mapped(&self) -> usize {
        self.lines.iter().filter(|l| l.inst.is_some()).count()
    }

    pub fn html(&self) -> String {
        let mut body = String::from("<div class=\"scroll mono\"><table>\n<tr><th>CLIF</th><th>offset</th><th>bytes</th><th>machine code</th></tr>\n");
        let mut start = 0;
        while start < self.lines.len() {
            let inst = self.lines[start].inst;
            let end = (start..self.lines.len()).find(|&i| self.lines[i].inst != inst).unwrap_or(self.lines.len());
            body.push_str("<tbody>\n");
            for index in start..end {
                let line = &self.lines[index];
                body.push_str("<tr>");
                if index == start {
                    let clif = inst.map_or_else(|| "<span class=\"none\">no CLIF instruction</span>".to_owned(), |i| escape(&self.insts[i]));
                    body.push_str(&format!("<td class=\"clif\" rowspan=\"{}\">{clif}</td>", end - start));
                }
                body.push_str(&format!("<td>{:04x}</td><td class=\"bytes\">{}</td><td>{}</td></tr>\n", line.offset, hex(&line.bytes), escape(&line.text)));
            }
            body.push_str("</tbody>\n");
            start = end;
        }
        body.push_str("</table></div>\n");
        html::page(&format!("Disassembly of {} ({})", self.name, self.triple), &[], CSS, &body, "")
    }
}

const CSS: &str = r#"
.scroll { overflow: auto; max-height: calc(100vh - 110px); }
table { border-collapse: collapse; }
th, td { border: 1px solid var(--border); padding: 0 8px; height: var(--row); white-space: pre; text-align: left; }
th { background: var(--head); position: sticky; top: 0; z-index: 1; }
td.clif { background: var(--head); vertical-align: top; }
td.bytes { color: var(--muted); }
span.none { color: var(--muted); }
tbody:hover td { outline: 2px solid var(--hl); outline-offset: -2px; }
"#;

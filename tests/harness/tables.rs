use clif_lsp::tables::{self, opcodes::OPCODES};
use cranelift_codegen::ir::Opcode;

pub fn check() -> Result<(), String> {
    let real = Opcode::all();
    if real.len() != OPCODES.len() {
        return Err(format!("opcode table has {} entries, cranelift has {}", OPCODES.len(), real.len()));
    }
    for op in real {
        let name = op.to_string();
        let info = tables::opcode(&name).ok_or_else(|| format!("opcode `{name}` missing from table"))?;
        let flags = [
            ("format", info.format == format!("{:?}", op.format())),
            ("terminator", info.terminator == op.is_terminator()),
            ("branch", info.branch == op.is_branch()),
            ("call", info.call == op.is_call()),
            ("ret", info.ret == op.is_return()),
            ("load", info.load == op.can_load()),
            ("store", info.store == op.can_store()),
            ("trap", info.trap == op.can_trap()),
            ("side_effects", info.side_effects == op.other_side_effects()),
        ];
        if let Some((field, _)) = flags.iter().find(|(_, same)| !same) {
            return Err(format!("opcode `{name}`: `{field}` differs from cranelift"));
        }
    }
    Ok(())
}

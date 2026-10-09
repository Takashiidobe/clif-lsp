pub mod opcodes;

pub struct OpcodeInfo {
    pub name: &'static str,
    pub signature: &'static str,
    pub format: &'static str,
    pub doc: &'static str,
    pub terminator: bool,
    pub branch: bool,
    pub call: bool,
    pub ret: bool,
    pub load: bool,
    pub store: bool,
    pub trap: bool,
    pub side_effects: bool,
}

pub fn opcode(name: &str) -> Option<&'static OpcodeInfo> {
    opcodes::OPCODES
        .binary_search_by_key(&name, |o| o.name)
        .ok()
        .map(|i| &opcodes::OPCODES[i])
}

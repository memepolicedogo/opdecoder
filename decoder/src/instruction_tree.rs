use core::panic;
use std::usize;
use std::{collections::HashMap, fmt};

use regex::Regex;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Instruction {
    pub opcode: String,
    #[serde(rename = "instruction")]
    pub text: String,
    #[serde(rename = "current_support")]
    pub x64: String,
    #[serde(rename = "legacy_support")]
    pub legacy: String,
    pub operands: Option<Vec<String>>,
    pub description: String,
}

#[derive(Eq, Hash, Clone, Copy, Debug)]
struct OpByte {
    code: u8,
    mask: u8,
    inv_code: u8,
    inv_mask: u8,
}

impl fmt::Display for OpByte {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02X}{:02X}{:02X}{:02X}",
            self.code, self.mask, self.inv_code, self.inv_mask
        )
    }
}

impl std::str::FromStr for OpByte {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 8 {
            return Err("Invalid length".into());
        }

        Ok(OpByte {
            code: u8::from_str_radix(&s[0..2], 16).map_err(|e| e.to_string())?,
            mask: u8::from_str_radix(&s[2..4], 16).map_err(|e| e.to_string())?,
            inv_code: u8::from_str_radix(&s[4..6], 16).map_err(|e| e.to_string())?,
            inv_mask: u8::from_str_radix(&s[6..8], 16).map_err(|e| e.to_string())?,
        })
    }
}

impl Serialize for OpByte {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for OpByte {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(D::Error::custom)
    }
}

impl Default for OpByte {
    fn default() -> Self {
        Self {
            code: 0,
            mask: 255,
            inv_code: 0,
            inv_mask: 0,
        }
    }
}

impl PartialEq for OpByte {
    fn eq(&self, other: &Self) -> bool {
        (self.code == other.code)
            && (self.mask == other.mask)
            && (self.inv_mask == other.inv_mask)
            && (self.inv_code == other.inv_code)
    }
    fn ne(&self, other: &Self) -> bool {
        (self.code != other.code)
            || (self.mask != other.mask)
            || (self.inv_mask != other.inv_mask)
            || (self.inv_code != other.inv_code)
    }
}

impl PartialEq<u8> for OpByte {
    fn eq(&self, other: &u8) -> bool {
        self.code == (other & self.mask)
            && (self.inv_mask == 0 || self.inv_code != (other & self.inv_mask))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Node {
    val: OpByte,
    instructions: Vec<Instruction>,
    children: HashMap<OpByte, usize>,
}

impl Node {
    fn get(&self, byte: &u8) -> Option<usize> {
        for key in self.children.keys() {
            if key == byte {
                return Some(*self.children.get(key).expect("THIS DUDE WATCHING PORN"));
            }
        }
        return None;
    }
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.val == other.val
    }

    fn ne(&self, other: &Self) -> bool {
        self.val != other.val
    }
}
impl<'a> PartialEq<OpByte> for &Node {
    fn eq(&self, other: &OpByte) -> bool {
        self.val == *other
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InstructionTree {
    nodes: Vec<Node>,
    root: usize,
    last: usize,
}

#[derive(Debug)]
pub struct InsTreeResponse<'a> {
    pub val: Vec<&'a Instruction>,
    pub bottom: bool,
}
const IGNORED_CODES: [&'static str; 13] = [
    "NP", "NFx", "cb", "cw", "cd", "cp", "co", "ct", "ib", "iw", "id", "io", "+i",
];

impl<'a> InstructionTree {
    fn parse_opcode(opcode: &String) -> Vec<OpByte> {
        let mut result = Vec::new();
        let components = opcode.split(' ');
        for byte in components {
            // Immediate/Code offset means we're at the end of the opcode
            if byte.starts_with('i') || byte.starts_with('c') {
                break;
            }
            if byte.len() == 2
                && let Ok(val) = u8::from_str_radix(byte, 16)
            {
                result.push(OpByte {
                    code: val,
                    ..Default::default()
                });
            } else if IGNORED_CODES.contains(&byte) {
                // Ignore
            } else if byte.contains("+") {
                // 0xXX+r[bwd]
                result.push(OpByte {
                    code: u8::from_str_radix(&byte[..2], 16).expect("Invalid Opcode"),
                    mask: 0b11111000,
                    ..Default::default()
                });
            } else if byte.starts_with("REX.") {
                if byte.ends_with('R') {
                    // REX.R
                    result.push(OpByte {
                        code: 0b01000000,
                        mask: 0b11111000,
                        ..Default::default()
                    });
                } else {
                    // REX.W
                    result.push(OpByte {
                        code: 0b01001000,
                        mask: 0b11111000,
                        ..Default::default()
                    });
                }
            } else if byte.starts_with('/') {
                // MOD | d | R/M
                if byte.ends_with('0') {
                    result.push(OpByte {
                        code: 0b00000000,
                        mask: 0b00111000,
                        ..Default::default()
                    });
                } else if byte.ends_with('1') {
                    result.push(OpByte {
                        code: 0b00001000,
                        mask: 0b00111000,
                        ..Default::default()
                    });
                } else if byte.ends_with('2') {
                    result.push(OpByte {
                        code: 0b00010000,
                        mask: 0b00111000,
                        ..Default::default()
                    });
                } else if byte.ends_with('3') {
                    result.push(OpByte {
                        code: 0b00011000,
                        mask: 0b00111000,
                        ..Default::default()
                    });
                } else if byte.ends_with('4') {
                    result.push(OpByte {
                        code: 0b00100000,
                        mask: 0b00111000,
                        ..Default::default()
                    });
                } else if byte.ends_with('5') {
                    result.push(OpByte {
                        code: 0b00101000,
                        mask: 0b00111000,
                        ..Default::default()
                    });
                } else if byte.ends_with('6') {
                    result.push(OpByte {
                        code: 0b00110000,
                        mask: 0b00111000,
                        ..Default::default()
                    });
                } else if byte.ends_with('7') {
                    result.push(OpByte {
                        code: 0b00111000,
                        mask: 0b00111000,
                        ..Default::default()
                    });
                }
            } else if byte.contains(':') {
                // 11:rrr:bbb type bytes
                let mut mask: u8 = 0;
                let mut code: u8 = 0;
                let mut inv_mask: u8 = 0;
                let mut inv_code: u8 = 0;
                let mut i = 7;
                let mut neg = false;
                for bit in byte.chars() {
                    match bit {
                        '!' => neg = true,
                        '1' => {
                            if neg {
                                inv_mask = inv_mask | (1 << i);
                                inv_code = inv_code | (1 << i);
                            } else {
                                mask = mask | (1 << i);
                                code = code | (1 << i);
                            }
                            i -= 1;
                        }
                        // If the required byte is null then we just have to update the mask to
                        // include it
                        '0' => {
                            if neg {
                                inv_mask = inv_mask | (1 << i);
                            } else {
                                mask = mask | (1 << i);
                            }
                            i -= 1;
                        }
                        'r' => i -= 1,
                        'b' => i -= 1,
                        _ => continue,
                    }
                }
                result.push(OpByte {
                    code,
                    mask,
                    inv_code,
                    inv_mask,
                });
            } else {
                println!("Unimplemented Byte: {:?}", byte);
                panic!("Implement my pages");
            }
        }
        return result;
    }

    pub fn from_json(json: &String) -> Self {
        let tables: Vec<Vec<Instruction>> = serde_json::from_str(json).expect("FUCK");
        if tables.len() == 0 {
            println!("Invalid JSON");
        }
        let mut result = Self {
            root: 0,
            nodes: vec![Node {
                val: OpByte {
                    code: 0,
                    mask: 0,
                    ..Default::default()
                },
                instructions: Vec::new(),
                children: HashMap::new(),
            }],
            last: 0,
        };
        for table in tables {
            for instruction in table {
                let path = InstructionTree::parse_opcode(&instruction.opcode);
                let mut node_index = result.root;
                for step in path {
                    // Iterate through every opcode byte in the instruction, creating new nodes as
                    // needed until we're at the end
                    let next_index =
                        if let Some(&child) = result.nodes[node_index].children.get(&step) {
                            child
                        } else {
                            let new_index = result.nodes.len();
                            result.nodes.push(Node {
                                val: step,
                                instructions: Vec::new(),
                                children: HashMap::new(),
                            });
                            result.nodes[node_index].children.insert(step, new_index);
                            new_index
                        };

                    node_index = next_index;
                    result.last = node_index;
                }
                // Add instruction to final node
                result.nodes[result.last].instructions.push(instruction);
                // Reset last
                result.last = result.root;
            }
        }

        return result;
    }

    // Traverse the tree from root using the opcode
    pub fn traverse(&mut self, opcode: &Vec<u8>) -> InsTreeResponse<'_> {
        self.last = self.root;
        let mut curr = self.root;
        for step in opcode {
            curr = self.nodes[self.last].get(step).unwrap_or(self.last);
            // If get failed return empty response
            if curr == self.last {
                return InsTreeResponse {
                    val: Vec::new(),
                    bottom: true,
                };
            }
            self.last = curr;
        }
        // If full path is traversed get relevent instructions and return
        return InsTreeResponse {
            val: self.gather_instructions(curr),
            bottom: self.nodes[curr].children.is_empty(),
        };
    }

    // Step down the tree by one byte/node
    // Like traverse but from self.last instead of root
    pub fn step(&mut self, byte: u8) -> InsTreeResponse<'_> {
        let curr = self.nodes[self.last].get(&byte);
        // If curr and last point to the same node then the given byte does not apply to an opcode
        if curr.is_none() {
            self.last = self.root;
            return InsTreeResponse {
                val: Vec::new(),
                bottom: true,
            };
        } else {
            let exp = curr.expect("Impossible");
            self.last = exp;
            return InsTreeResponse {
                // Get all possible instructions
                val: self.gather_instructions(exp),
                // If last node has no children we're at the bottom, otherwise false
                bottom: self.nodes[exp].children.is_empty(),
            };
        }
    }

    // Recursively get all instructions from self.last down
    pub fn gather_instructions(&self, index: usize) -> Vec<&Instruction> {
        let mut response = Vec::new();
        let curr = &self.nodes[index];
        response.extend(&curr.instructions);
        for (_, node) in &curr.children {
            response.extend(self.gather_instructions(*node));
        }
        return response;
    }

    pub fn reset(&mut self) {
        self.last = self.root;
    }
}

pub enum OpType {
    Reg, // Register
    Mem, // Memory
    Imm, // Immediate
}

pub struct Opperand {
    pub kind: OpType,
}
pub enum ArchSize {
    I16,
    I32,
    I64,
}

pub struct Context {
    pub size: ArchSize,
    pub one: u8,
    pub two: u8,
    pub op_override: bool,
    pub addr_override: bool,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            size: ArchSize::I64,
            one: 0,
            two: 0,
            op_override: false,
            addr_override: false,
        }
    }
}

pub struct ByteString {
    code: Vec<u8>,
    curr: usize,
}

impl ByteString {
    // Advance the cursor by some number of bytes
    pub fn advance(&mut self, by: usize) -> bool {
        self.curr += by;
        if self.curr >= self.code.len() {
            self.curr -= by;
            return false;
        }
        true
    }

    // Remove bytes behind cursor and reset cursor value
    pub fn trim(&mut self) {
        self.code.drain(..self.curr);
        self.curr = 0;
    }

    // Add more bytes to the bytestring
    pub fn append(&mut self, bytes: &Vec<u8>) {
        self.code.extend(bytes);
    }

    pub fn push(&mut self, byte: u8) {
        self.code.push(byte);
    }
}

pub struct InstructionResponse {
    pub val: Option<Instruction>,
    pub offset: usize,
}

pub struct OperandResponse {
    pub val: Option<Vec<String>>,
    pub offset: usize,
}

pub struct ParseResponse {
    pub instruction: Option<Instruction>,
    pub operands: Option<Vec<String>>,
    pub offset: usize,
}

impl ParseResponse {
    pub fn pretty_print(&self) {
        if self.instruction.is_none() {
            println!("No instruction");
            return;
        }
        let mut full_str = String::new();
        let ins = self.instruction.as_ref().unwrap();
        // Get the base instruction name sans ops
        full_str.push_str(ins.text.split(' ').collect::<Vec<_>>()[0]);
        if self.operands.is_none() {
            println!("{}", full_str);
            return;
        }
        for op in self.operands.as_ref().unwrap() {
            // Leading space and trailing comma for each
            full_str.push(' ');
            full_str.push_str(op);
            full_str.push(',');
        }
        // Remove trailing comma
        full_str.pop();
        println!("{}", full_str);
    }
}

impl Default for OperandResponse {
    fn default() -> Self {
        Self {
            val: None,
            offset: 0,
        }
    }
}

pub struct Decoder {
    pub context: Context,
    pub tree: InstructionTree,
}

const MAX_PREFIX: usize = 4;
const MAX_WIDTH: usize = MAX_PREFIX + 8;
const BASE_REGS_REX_EXTENDED: [&str; 16] = [
    "A", "C", "D", "B", "SP", "BP", "SI", "DI", "R8", "R9", "R10", "R11", "R12", "R13", "R14",
    "R15",
];

impl Decoder {
    pub fn parse(&mut self, bytestring: &Vec<u8>) -> ParseResponse {
        let instruction = self.parse_instruction(bytestring);
        let operands = self.parse_operands(&instruction, bytestring);
        ParseResponse {
            instruction: instruction.val,
            operands: operands.val,
            offset: instruction.offset + operands.offset,
        }
    }

    pub fn parse_operands(
        &self,
        ins: &InstructionResponse,
        bytestring: &Vec<u8>,
    ) -> OperandResponse {
        match self.context.size {
            ArchSize::I32 => return self.parse_operands_i32(ins, bytestring),
            ArchSize::I64 => return self.parse_operands_i64(ins, bytestring),
            _ => {
                return OperandResponse {
                    ..Default::default()
                };
            }
        };
        return OperandResponse {
            ..Default::default()
        };
    }

    fn parse_operands_i32(
        &self,
        ins: &InstructionResponse,
        bytestring: &Vec<u8>,
    ) -> OperandResponse {
        return OperandResponse {
            ..Default::default()
        };
    }

    fn parse_operands_i64(
        &self,
        ins: &InstructionResponse,
        bytestring: &Vec<u8>,
    ) -> OperandResponse {
        let instruction = ins.val.as_ref().unwrap();
        // Get this out of the way first
        if instruction.operands.is_none() {
            return OperandResponse {
                ..Default::default()
            };
        }
        let mut rex_w = false;
        let mut rex_r = 0;
        let mut rex_x = 0;
        let mut rex_b = 0;
        if instruction.opcode.starts_with("REX") {
            if bytestring[0] & 0b11110000 != 0b01000000 {
                panic!("Invalid REX prefix");
            }
            // Parse REX prefix
            rex_w = (bytestring[0] & 0b00001000) != 0;
            rex_r = (bytestring[0] & 0b00000100) << 1;
            rex_x = (bytestring[0] & 0b00000010) << 2;
            rex_b = (bytestring[0] & 0b00000001) << 3;
        }

        let mut offset = ins.offset;
        let mut op_strings: Vec<String> = Vec::new();
        for op in instruction.operands.as_ref().unwrap() {
            // N/A means we're at the last one
            if op == "N/A" {
                break;
            }
            let mut op_str = String::new();
            // Handle ModRM variations
            // already almost 150 lines, now imagine if I was doing VEX stuff too
            if op.starts_with("ModRM") {
                // First byte of the opperands
                let modrm = bytestring[ins.offset];
                let mode = (modrm & 0b11000000) >> 6;
                let rm = modrm & 0b00000111;
                let reg = (modrm & 0b00111000) >> 3;
                // ModR/M:reg or ModR/M:r/m and mod = 3 means we're dealing with a register
                if op.contains("reg") || (op.contains("r/m") && mode == 3) {
                    // Operand is just a register
                    // Get name based on size
                    op_str = Decoder::format_reg(
                        // Smallest name of reg, e.g. A or R13 or SP
                        BASE_REGS_REX_EXTENDED[usize::from(rex_b | rm)],
                        // ADD r/m8, r8 => ["ADD r/m8", " r8"]
                        // offset - ins.offset = operand index
                        instruction.text.split(',').collect::<Vec<_>>()[offset - ins.offset],
                    );
                    offset += 1;
                } else {
                    // Operand is memory
                    // This is the proper way of formatting memory accesses
                    // MASM can get bent
                    op_str.insert(0, '[');
                    // Special cases
                    // Increment offset because we aren't accessing the
                    offset += 1;
                    // Literal displacement
                    // 0x67 prefix and mod == 0b00 and rm == 0b101 -> Just a 32 bit displacement, zero
                    // extended
                    if (self.context.addr_override && mode == 0 && rm == 5) {
                        // Next 4 bytes are displacements
                        op_str = Decoder::format_imm(bytestring, offset, 4);
                        // Add brackets
                        op_str.insert(0, '[');
                        op_str.push(']');
                    } else {
                        // If mod != 0b11 and R/M == 100 then there is an SIB byte procededing the modrm byte
                        // This is true independant of the REX prefix
                        if mode != 3 && rm == 4 {
                            // SIB Stuff
                            offset += 1;
                            let scale = bytestring[offset] & 0b11000000 >> 6;
                            let index = bytestring[offset] & 0b00111000 >> 3;
                            let base = bytestring[offset] & 0b00000111;
                            if (rex_x | index) == 4 {
                                // RSP is not to be used as an index
                            } else {
                                // Get index register
                                op_str += &Decoder::format_reg(
                                    BASE_REGS_REX_EXTENDED[usize::from(rex_x | index)],
                                    "r64",
                                );
                                // Apply scaling
                                match scale {
                                    1 => {
                                        op_str.push('*');
                                        op_str.push('2');
                                        op_str.push('+');
                                    }
                                    2 => {
                                        op_str.push('*');
                                        op_str.push('4');
                                        op_str.push('+');
                                    }
                                    3 => {
                                        op_str.push('*');
                                        op_str.push('8');
                                        op_str.push('+');
                                    }
                                    _ => op_str.push('+'),
                                }
                            }
                            // Add base register
                            if base != 5 {
                                op_str += &Decoder::format_reg(
                                    BASE_REGS_REX_EXTENDED[usize::from(rex_b | base)],
                                    "r64",
                                );
                            } else {
                                // When base is 0b101 it means either it's based on RBP or a
                                // displacement, depending on mod
                                match mode {
                                    // Just disp32
                                    0 => {
                                        op_str.push('+');
                                        op_str
                                            .push_str(&Decoder::format_imm(bytestring, offset, 4));
                                        op_str.push(']');
                                    }
                                    // disp8 + ebp
                                    1 => {
                                        op_str.push('+');
                                        op_str
                                            .push_str(&Decoder::format_imm(bytestring, offset, 1));
                                        op_str.push('+');
                                        op_str.push_str("RBP");
                                        op_str.push(']');
                                    }
                                    // disp32 + ebp
                                    // all this to enable C local variabes. Very cool
                                    2 => {
                                        op_str.push('+');
                                        op_str
                                            .push_str(&Decoder::format_imm(bytestring, offset, 4));
                                        op_str.push('+');
                                        op_str.push_str("RBP");
                                        op_str.push(']');
                                    }
                                    _ => {}
                                }
                            }
                        } else {
                            // This is where the normal ModR/M parsing begins
                            // Get base register
                            op_str +=
                            // Base reg is determined by REX.B and R/M bits, and bc we're in 64 bit mode it
                            // has to be 64 bit, hence passing "r64" literal rather than using anything
                            // from the instruction
                            &Decoder::format_reg(BASE_REGS_REX_EXTENDED[usize::from(rex_b | rm)], "r64");
                            // op_str = "[{reg}"
                            // Now we find any displacement
                            match mode {
                                // No IMM displacement
                                0 => {
                                    op_str.push(']');
                                }
                                // 8 bit displacement
                                1 => {
                                    op_str.push('+');
                                    op_str.push_str(&Decoder::format_imm(bytestring, offset, 1));
                                    op_str.push(']');
                                }
                                // 32 bit displacement
                                2 => {
                                    op_str.push('+');
                                    op_str.push_str(&Decoder::format_imm(bytestring, offset, 4));
                                    op_str.push(']');
                                }
                                _ => {}
                            }
                        }
                    }
                }
            } else if op.starts_with("imm") || op.starts_with("disp") {
                // Immediate
                // We have to get the size from the instruction part not the operands
                let imm_str = instruction.text.split(',').collect::<Vec<_>>()[offset - ins.offset];
                if imm_str.ends_with("64") {
                    op_str = Decoder::format_imm(bytestring, offset, 8);
                    offset += 8;
                } else if imm_str.ends_with("32") {
                    op_str = Decoder::format_imm(bytestring, offset, 4);
                    offset += 4;
                } else if imm_str.ends_with("16") {
                    op_str = Decoder::format_imm(bytestring, offset, 2);
                    offset += 2;
                } else if imm_str.ends_with("8") {
                    op_str = Decoder::format_imm(bytestring, offset, 1);
                    offset += 1;
                }
            }
            op_strings.push(op_str);
        }
        return OperandResponse {
            val: Some(op_strings),
            offset: offset - ins.offset,
        };
    }

    fn format_imm(bytestring: &Vec<u8>, offset: usize, count: usize) -> String {
        // Number of bytes
        let mut i = count;
        let mut val: u64 = 0;
        while i > 0 {
            i -= 1;
            // as i decreases we step further in the bytestring, and shift less
            // immediate values are ordered most significant byte first
            // The byte we pull from the bytetring has to be converted to a u64 first
            // or it'll overflow to zero
            val += u64::from(bytestring[offset + (count - (i + 1))]) << (i * 8);
        }
        val.to_string()
    }

    fn format_reg(base: &str, ins_str: &str) -> String {
        let mut result = base.to_string();
        if ins_str.contains("r8") || ins_str.contains("r/m8") {
            if result.starts_with("R") {
                return result + "B";
            } else {
                return result + "L";
            }
        } else if ins_str.contains("r16") || ins_str.contains("r/m16") {
            if result.starts_with("R") {
                return result + "W";
            } else if result.len() == 1 {
                return result + "X";
            } else {
                return result;
            }
        } else if ins_str.contains("r32") || ins_str.contains("r/m32") {
            if result.starts_with("R") {
                return result + "D";
            } else if result.len() == 1 {
                result.insert(0, 'E');
                return result + "X";
            } else {
                result.insert(0, 'E');
                return result;
            }
        } else if ins_str.contains("r64") || ins_str.contains("r/m64") {
            if result.starts_with("R") {
                return result;
            } else if result.len() == 1 {
                result.insert(0, 'R');
                return result + "X";
            } else {
                result.insert(0, 'R');
                return result;
            }
        }
        return result;
    }

    fn parse_modrm(&self, bytestring: Vec<u8>) {
        let byte = bytestring[0];
        let mode = (byte & 0b1100000) >> 6;
        let rm = byte & 0b00000111;
        let reg = (byte & 0b00111000) >> 3;
        // If mod != 0b11 and R/M == 100 then there is an SIB byte procededing the modrm byte
        if mode != 3 && rm == 4 {
            let sib = bytestring[1];
        }
        match mode {
            0 => println!("Zero"),
            1 => println!("Zero"),
            2 => println!("Zero"),
            3 => println!("Zero"),
            _ => panic!("Mod field had invalid value"),
        }
    }

    fn calc_opcode_size(opcode: &String) -> usize {
        let opperands = Regex::new("(/[r0-7])|(^[icr][bwdo])").unwrap();
        let mut i = 0;
        for byte in opcode.split(' ').into_iter() {
            // If byte is /{digit} or /r etc, that describes an opperand, and everthing following
            // must nessecarrily be an operand too
            if opperands.is_match(byte) {
                break;
            }
            i += 1;
        }
        return i;
    }

    pub fn parse_instruction(&mut self, bytestring: &Vec<u8>) -> InstructionResponse {
        let mut offset: usize = 0;
        let mut byte: u8;
        let mut prefix = Vec::new();
        let mut opcode = Vec::new();
        // Reset Context
        self.tree.reset();
        self.context.one = 0;
        self.context.two = 0;
        self.context.op_override = false;
        self.context.addr_override = false;
        // Step until we bottom out
        // If no instructions parse one byte as prefix, step again
        // Continue until nothing for 4th prefix byte
        // if still nothing return empty vec and 1 offset (try again 1 byte ahead)
        let mut prefix_count = 0;
        let ins = 'parent: loop {
            self.tree.reset();
            for i in (prefix_count..MAX_WIDTH) {
                let mut rep = self.tree.step(bytestring[i]);
                if rep.bottom && rep.val.is_empty() {
                    prefix_count += 1;
                    break;
                } else if rep.bottom {
                    // We've found at least one match
                    prefix.extend_from_slice(&bytestring[..prefix_count]);
                    opcode.extend_from_slice(&bytestring[prefix_count..i]);
                    break 'parent rep.val;
                }
            }
            if prefix_count > 4 {
                break Vec::new();
            }
        };
        // If we got nothing we do nothing
        if ins.is_empty() {
            return InstructionResponse {
                val: None,
                offset: 1,
            };
        }
        // Figure out the prefixes
        for byte in prefix {
            // If byte isn't in range to be a valid prefix then escape
            if byte < 0x26 || byte > 0xf3 {
                break;
            } else if byte >= 0xf0 {
                self.context.one = byte;
            } else if byte == 0x66 {
                self.context.op_override = true;
            } else if byte == 0x67 {
                self.context.addr_override = true;
            } else {
                self.context.two = match byte {
                    0x2e => 0x2e,
                    0x36 => 0x36,
                    0x3e => 0x3e,
                    0x26 => 0x26,
                    0x64 => 0x64,
                    0x65 => 0x65,
                    _ => 0,
                };
                if self.context.two == 0 {
                    break;
                }
            }
        }
        // Context is probably accurate now idk
        // Now we have to do conflict resolution and ensure that the prefixes and the instruction
        // match
        let mut valids = Vec::new();
        for instruction in ins {
            // Check if instruction matches the vibes
            if instruction.opcode.contains("NP")
                && (self.context.one == 0xf2
                    || self.context.one == 0xf3
                    || self.context.op_override)
            {
                //Invalid
                continue;
            } else if instruction.opcode.contains("NFx")
                && (self.context.one == 0xf2 || self.context.one == 0xf3)
            {
                //Invalid
                continue;
            }
            valids.push(instruction);
        }

        // Are any invalid on target arch?
        let mut i = 0;
        while i < valids.len() {
            match self.context.size {
                ArchSize::I16 => {
                    if valids[i].legacy != "V" {
                        valids.remove(i);
                    } else {
                        i += 1;
                    }
                }
                ArchSize::I32 => {
                    if valids[i].legacy != "V" {
                        valids.remove(i);
                    } else {
                        i += 1;
                    }
                }
                ArchSize::I64 => {
                    if valids[i].x64 != "V" {
                        valids.remove(i);
                    } else {
                        i += 1;
                    }
                }
            };
        }
        if valids.is_empty() {
            return InstructionResponse {
                val: None,
                offset: 1,
            };
        } else if valids.len() == 1 {
            return InstructionResponse {
                val: Some(valids[0].clone()),
                offset: prefix_count + Decoder::calc_opcode_size(&valids[0].opcode),
            };
        // Still may have multiple if entries rely on prefixes to infer size
        } else {
            for ins in valids {
                //  Pull largest digit from name
                // Or maybe just
                match self.context.size {
                    ArchSize::I16 => {
                        if (self.context.op_override || self.context.addr_override)
                            && ins.text.contains("8")
                        {
                            return InstructionResponse {
                                val: Some(ins.clone()),
                                offset: prefix_count + Decoder::calc_opcode_size(&ins.opcode),
                            };
                        } else if (!self.context.op_override && !self.context.addr_override)
                            && ins.text.contains("16")
                        {
                            return InstructionResponse {
                                val: Some(ins.clone()),
                                offset: prefix_count + Decoder::calc_opcode_size(&ins.opcode),
                            };
                        }
                    }
                    ArchSize::I32 => {
                        if (self.context.op_override || self.context.addr_override)
                            && ins.text.contains("16")
                        {
                            return InstructionResponse {
                                val: Some(ins.clone()),
                                offset: prefix_count + Decoder::calc_opcode_size(&ins.opcode),
                            };
                        } else if (!self.context.op_override && !self.context.addr_override)
                            && ins.text.contains("32")
                        {
                            return InstructionResponse {
                                val: Some(ins.clone()),
                                offset: prefix_count + Decoder::calc_opcode_size(&ins.opcode),
                            };
                        }
                    }
                    ArchSize::I64 => {
                        if (self.context.op_override || self.context.addr_override)
                            && ins.text.contains("16")
                        {
                            return InstructionResponse {
                                val: Some(ins.clone()),
                                offset: prefix_count + Decoder::calc_opcode_size(&ins.opcode),
                            };
                        } else if (!self.context.op_override && !self.context.addr_override)
                            && (ins.text.contains("32") || ins.text.contains("64"))
                        {
                            return InstructionResponse {
                                val: Some(ins.clone()),
                                offset: prefix_count + Decoder::calc_opcode_size(&ins.opcode),
                            };
                        }
                    }
                };
            }
        }
        // What possibly can be here?
        // ??
        panic!("At the disco");

        // If instruction is invalid
        return InstructionResponse {
            val: None,
            offset: 1,
        };
    }
}

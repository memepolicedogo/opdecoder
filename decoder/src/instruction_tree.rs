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

#[derive(Debug)]
pub enum ArchSize {
    I16,
    I32,
    I64,
}

#[derive(Debug)]
pub struct Rex {
    pub w: bool,
    pub r: u8,
    pub b: u8,
    pub x: u8,
}

impl Rex {
    fn from(value: u8) -> Self {
        Self {
            w: (value & 0b00001000) != 0,
            r: (value & 0b00000100) << 1,
            x: (value & 0b00000010) << 2,
            b: (value & 0b00000001) << 3,
        }
    }
}

pub struct Context {
    pub size: ArchSize,
    pub one: u8,
    pub two: u8,
    pub op_override: bool,
    pub addr_override: bool,
    pub rex: Option<Rex>,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            size: ArchSize::I64,
            one: 0,
            two: 0,
            op_override: false,
            addr_override: false,
            rex: None,
        }
    }
}

pub struct ByteString {
    pub code: Vec<u8>,
    pub curr: usize,
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

    // Add a single byte to the bytestring
    pub fn push(&mut self, byte: u8) {
        self.code.push(byte);
    }

    // Get the byte at the specific index
    pub fn get_at(&self, index: usize) -> u8 {
        self.code[index]
    }

    // Get the byte at the index relative to curr
    pub fn get_offset(&self, offset: isize) -> u8 {
        let mut index = self.curr as isize + offset;
        if index as usize >= self.code.len() {
            0
        } else {
            self.code[index as usize]
        }
    }

    // Get the byte at curr
    pub fn get(&self) -> u8 {
        self.code[self.curr]
    }

    // Increase curr and get next byte
    pub fn step(&mut self) -> u8 {
        self.curr += 1;
        self.code[self.curr]
    }

    pub fn inc(&mut self) -> bool {
        if self.is_end() {
            false
        } else {
            self.curr += 1;
            true
        }
    }

    pub fn dec(&mut self) {
        self.curr -= 1;
    }

    pub fn is_end(&mut self) -> bool {
        self.curr >= (self.code.len() - 1)
    }

    pub fn get_slice(&mut self, from: usize, to: usize) -> &[u8] {
        &self.code[from..to]
    }

    pub fn get_slice_offset(&mut self, from: isize, to: isize) -> &[u8] {
        // man this is ugly
        &self.code[((self.curr as isize + from) as usize)..((self.curr as isize + to) as usize)]
    }
}

#[derive(Debug)]
pub struct InstructionResponse {
    pub val: Option<Instruction>,
    pub size: usize,
}

#[derive(Debug)]
pub struct OperandResponse {
    pub val: Option<Vec<String>>,
    pub size: usize,
}

pub struct ParseResponse {
    pub instruction: Option<Instruction>,
    pub operands: Option<Vec<String>>,
    pub bytes: Option<Vec<u8>>,
}

impl fmt::Display for ParseResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        return if self.bytes.is_none() {
            writeln!(f, "Failed to parse")
        } else if self.instruction.is_none() {
            writeln!(f, "{:02X}", self.bytes.as_ref().unwrap()[0])
        } else {
            let mut full_str = String::new();
            let ins = self.instruction.as_ref().unwrap();
            // Get the base instruction name sans ops
            full_str.push_str(ins.text.split(' ').collect::<Vec<_>>()[0]);
            if self.operands.is_none() {
                writeln!(f, "{}", full_str);
            }
            for op in self.operands.as_ref().unwrap() {
                // Leading space and trailing comma for each
                full_str.push(' ');
                full_str.push_str(op);
                full_str.push(',');
            }
            // Remove trailing comma
            full_str.pop();
            writeln!(f, "{}", full_str)
        };
    }
}

impl ParseResponse {
    pub fn pretty_print(&self) {
        if self.bytes.is_none() {
            println!("Failed to parse");
            return;
        } else if self.instruction.is_none() {
            self.print_bytes();
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

    pub fn print_bytes(&self) {
        if self.bytes.is_none() {
            return;
        }
        for byte in self.bytes.as_ref().unwrap() {
            print!("{:02X} ", byte);
        }
        println!("");
        return;
    }
}

impl Default for OperandResponse {
    fn default() -> Self {
        Self { val: None, size: 0 }
    }
}

pub struct Decoder {
    pub context: Context,
    pub tree: InstructionTree,
    pub code: ByteString,
}

const MAX_PREFIX: usize = 4;
const MAX_WIDTH: usize = MAX_PREFIX + 8;
const BASE_REGS_REX_EXTENDED: [&str; 16] = [
    "A", "C", "D", "B", "SP", "BP", "SI", "DI", "R8", "R9", "R10", "R11", "R12", "R13", "R14",
    "R15",
];
const BASE_REGS: [&str; 8] = ["A", "C", "D", "B", "AH", "CH", "DH", "BH"];

impl Decoder {
    pub fn parse_n_print(&mut self) {
        while !self.code.is_end() {
            let inc = self.parse_one();
            inc.print_bytes();
            inc.pretty_print();
        }
    }

    //
    pub fn parse(&mut self) -> Vec<ParseResponse> {
        let mut responses = Vec::new();
        while !self.code.is_end() {
            responses.push(self.parse_one());
        }
        responses
    }
    pub fn parse_one(&mut self) -> ParseResponse {
        // If no code is left return nothing
        if self.code.is_end() {
            return ParseResponse {
                instruction: None,
                operands: None,
                bytes: None,
            };
        }
        let instruction = self.parse_instruction();
        if instruction.val.is_none() {
            let byte = self.code.step();
            // Increment code and return nothing (invalid instruction)
            return ParseResponse {
                instruction: None,
                operands: None,
                bytes: Some(vec![byte]),
            };
        }
        let operands = self.parse_operands(&instruction);
        let start_offset = -((instruction.size + operands.size) as isize);
        ParseResponse {
            instruction: instruction.val,
            operands: operands.val,
            bytes: Some(Vec::from(self.code.get_slice_offset(start_offset, 0))),
        }
    }

    pub fn parse_operands(&mut self, ins: &InstructionResponse) -> OperandResponse {
        match self.context.size {
            ArchSize::I32 => return self.parse_operands_i32(ins),
            ArchSize::I64 => return self.parse_operands_i64(ins),
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

    fn parse_operands_i32(&self, ins: &InstructionResponse) -> OperandResponse {
        return OperandResponse {
            ..Default::default()
        };
    }

    fn parse_operands_i64(&mut self, ins: &InstructionResponse) -> OperandResponse {
        let instruction = ins.val.as_ref().unwrap();
        let mut size = 0;
        // Get this out of the way first
        if instruction.operands.is_none() {
            return OperandResponse {
                ..Default::default()
            };
        }
        //  modrm is the first byte of ops, if there is no modrm for this instruction this is
        //  ignored
        let modrm = self.code.get();
        let mut rex_w = false;
        let mut rex_r = 0;
        let mut rex_x = 0;
        let mut rex_b = 0;
        if self.context.rex.is_some() {
            // Parse REX prefix
            rex_w = self.context.rex.as_ref().unwrap().w;
            rex_r = self.context.rex.as_ref().unwrap().r;
            rex_x = self.context.rex.as_ref().unwrap().x;
            rex_b = self.context.rex.as_ref().unwrap().b;
        }

        // Offset is the operand #, size is the amount of bytes
        // e.g. modrm w/ sib, imm64 has 2 opperands but is 10 bytes
        let mut offset = 0;
        let mut op_strings: Vec<String> = Vec::new();
        for op in instruction.operands.as_ref().unwrap() {
            let op_in_code = Regex::new("\\+r[bwdo]").unwrap();
            // N/A means we're at the last one
            if op == "N/A" {
                break;
            }
            let mut op_str = String::new();
            // Handle ModRM variations
            // already almost 150 lines, now imagine if I was doing VEX stuff too
            if op.starts_with("ModRM") {
                // The modrm byte is only one byte, but it may be used for multiple operands
                // so we have to be careful not to double count
                if size == 0 {
                    size += 1;
                    self.code.inc();
                }
                // First byte of the opperands
                let mode = (modrm & 0b11000000) >> 6;
                let rm = modrm & 0b00000111;
                let reg = (modrm & 0b00111000) >> 3;
                let loc_index = if op.contains("reg") {
                    (rex_r | reg) as usize
                } else {
                    (rex_b | rm) as usize
                };
                // ModR/M:reg or ModR/M:r/m and mod = 3 means we're dealing with a register
                if op.contains("reg") || (op.contains("r/m") && mode == 3) {
                    // Operand is just a register
                    // Get name based on size
                    op_str = Decoder::format_reg(
                        loc_index,
                        // ADD r/m8, r8 => ["ADD r/m8", " r8"]
                        // offset = operand index
                        if rex_r != 1 {
                            instruction.text.split(',').collect::<Vec<_>>()[offset]
                        } else {
                            "r64"
                        },
                    );
                } else {
                    // Operand is memory
                    // This is the proper way of formatting memory accesses
                    // MASM can get bent
                    op_str.insert(0, '[');
                    // Special cases
                    // Increment offset because we aren't accessing the
                    // Literal displacement
                    // 0x67 prefix and mod == 0b00 and rm == 0b101 -> Just a 32 bit displacement, zero
                    // extended
                    if (self.context.addr_override && mode == 0 && rm == 5) {
                        // Next 4 bytes are displacements
                        op_str = self.format_imm(4);
                        // Add brackets
                        op_str.insert(0, '[');
                        op_str.push(']');
                    } else {
                        // If mod != 0b11 and R/M == 100 then there is an SIB byte procededing the modrm byte
                        // This is true independant of the REX prefix
                        if mode != 3 && rm == 4 && op.contains("r/m") {
                            // SIB Stuff
                            let sib = self.code.get();
                            self.code.inc();
                            // SIB doesn't change the offset but it does change the size
                            size += 1;
                            let scale = (sib & 0b11000000) >> 6;
                            let index = (sib & 0b00111000) >> 3;
                            let base = sib & 0b00000111;
                            if (rex_x | index) == 4 {
                                // RSP is not to be used as an index
                            } else {
                                // Get index register
                                op_str += &Decoder::format_reg((rex_x | index) as usize, "r64");
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
                                op_str += &Decoder::format_reg(loc_index, "r64");
                            } else {
                                // When base is 0b101 it means either it's based on RBP or a
                                // displacement, depending on mod
                                match mode {
                                    // Just disp32
                                    0 => {
                                        op_str.push_str(&self.format_imm(4));
                                        size += 4;
                                        op_str.push(']');
                                    }
                                    // disp8 + ebp
                                    1 => {
                                        op_str.push_str(&self.format_imm(1));
                                        size += 1;
                                        op_str.push('+');
                                        op_str.push_str("RBP");
                                        op_str.push(']');
                                    }
                                    // disp32 + ebp
                                    // all this to enable C local variabes. Very cool
                                    2 => {
                                        op_str.push_str(&self.format_imm(4));
                                        size += 4;
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
                            &Decoder::format_reg(loc_index, "r64");
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
                                    op_str.push_str(&self.format_imm(1));
                                    op_str.push(']');
                                    size += 1;
                                }
                                // 32 bit displacement
                                2 => {
                                    op_str.push('+');
                                    op_str.push_str(&self.format_imm(4));
                                    op_str.push(']');
                                    size += 4;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            } else if op.starts_with("imm")
                || op.starts_with("disp")
                || op.starts_with("rel")
                || op == "Offset"
            {
                // Immediate
                // We have to get the size from the instruction part not the operands
                let imm_str = instruction.text.split(',').collect::<Vec<_>>()[offset];
                if imm_str.ends_with("64") {
                    op_str = self.format_imm(8);
                    size += 8;
                } else if imm_str.ends_with("32") {
                    op_str = self.format_imm(4);
                    size += 4;
                } else if imm_str.ends_with("16") {
                    op_str = self.format_imm(2);
                    size += 2;
                } else if imm_str.ends_with("8") {
                    op_str = self.format_imm(1);
                    size += 1;
                }
            } else if op.starts_with("opcode") {
                // Get last byte of instruction
                let register = (self.code.get_offset(-((size + 1) as isize)) & 0b00000111) | rex_b;
                op_str = Decoder::format_reg(
                    // Smallest name of reg, e.g. A or R13 or SP
                    register as usize,
                    // ADD r/m8, r8 => ["ADD r/m8", " r8"]
                    // offset = operand index
                    if rex_r != 1 {
                        instruction.text.split(',').collect::<Vec<_>>()[offset]
                    } else {
                        "r64"
                    },
                );
            }
            op_strings.push(op_str);
            offset += 1;
        }
        return OperandResponse {
            val: Some(op_strings),
            size,
        };
    }

    fn format_imm(&mut self, count: usize) -> String {
        let mut i = 0;
        let mut val: u64 = 0;
        while i < count {
            val += (self.code.get() as u64) << (i * 8);
            self.code.inc();
            i += 1;
        }
        format!("0x{:02X}", val)
    }

    fn format_reg(index: usize, ins_str: &str) -> String {
        let base = BASE_REGS_REX_EXTENDED[index];
        let mut result = base.to_string();
        let mut size = 64;
        if ins_str.contains("r8") || ins_str.contains("r/m8") {
            size = 8;
        } else if ins_str.contains("r16") || ins_str.contains("r/m16") {
            size = 16;
        } else if ins_str.contains("r32") || ins_str.contains("r/m32") {
            size = 32;
        } else if ins_str.contains("r64") || ins_str.contains("r/m64") {
            size = 64;
        }
        match size {
            64 => {
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
            32 => {
                if result.starts_with("R") {
                    return result + "D";
                } else if result.len() == 1 {
                    result.insert(0, 'E');
                    return result + "X";
                } else {
                    result.insert(0, 'E');
                    return result;
                }
            }
            16 => {
                if result.starts_with("R") {
                    return result + "W";
                } else if result.len() == 1 {
                    return result + "X";
                } else {
                    return result;
                }
            }
            8 => {
                if result.starts_with("R") {
                    return result + "B";
                } else {
                    return result + "L";
                }
            }
            _ => return result,
        }
        return result;
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

    pub fn parse_instruction(&mut self) -> InstructionResponse {
        let mut size: usize = 0;
        let mut byte: u8;
        let mut prefix = Vec::new();
        let mut opcode = Vec::new();
        // Reset Context
        self.tree.reset();
        self.context.rex = None;
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
                byte = self.code.get_offset(i as isize);
                let mut rep = self.tree.step(byte);
                if rep.bottom && rep.val.is_empty() {
                    prefix_count += 1;
                    break;
                } else if rep.bottom {
                    // We've found at least one match
                    // Iterate and handle prefix bytes
                    for _ in 0..prefix_count {
                        prefix.push(self.code.get());
                        self.code.inc();
                    }
                    size = ((i + 1) - prefix_count);
                    for _ in 0..size {
                        opcode.push(self.code.get());
                        self.code.inc();
                    }
                    break 'parent rep.val;
                }
            }
            if prefix_count > MAX_PREFIX {
                break Vec::new();
            }
        };
        if ins.is_empty() {
            return InstructionResponse { val: None, size: 1 };
        }
        // Figure out the prefixes
        for byte in prefix {
            // If byte isn't in range to be a valid prefix then escape
            if (byte & 0b11110000) == 0b01000000 {
                self.context.rex = Some(Rex::from(byte));
            } else if byte < 0x26 || byte > 0xf3 {
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
        if (opcode[0] & 0b11110000) == 0b01000000 {
            self.context.rex = Some(Rex::from(opcode[0]));
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
            return InstructionResponse { val: None, size: 0 };
        } else {
            // Adjust size
            // any /digit references a field in the ModR/M byte but based on the logic I have
            // implemented it is counted as part of the instruction AND the opcode without this
            // adjustment
            let re = Regex::new("/[0-7]").unwrap();
            if re.is_match(&valids[0].opcode) {
                size -= 1;
                self.code.dec();
            }
        }
        if valids.len() == 1 {
            return InstructionResponse {
                val: Some(valids[0].clone()),
                size,
            };
        // Still may have multiple if entries rely on prefixes to infer size
        } else {
            i = 0;
            while i < valids.len() {
                //  Pull largest digit from name
                // Or maybe just
                match self.context.size {
                    ArchSize::I16 => {
                        if (self.context.op_override || self.context.addr_override)
                            && valids[i].text.contains("8")
                        {
                            return InstructionResponse {
                                val: Some(valids[i].clone()),
                                size,
                            };
                        } else if (!self.context.op_override && !self.context.addr_override)
                            && valids[i].text.contains("16")
                        {
                            return InstructionResponse {
                                val: Some(valids[i].clone()),
                                size,
                            };
                        }
                    }
                    ArchSize::I32 => {
                        if (self.context.op_override || self.context.addr_override)
                            && valids[i].text.contains("16")
                        {
                            return InstructionResponse {
                                val: Some(valids[i].clone()),
                                size,
                            };
                        } else if (!self.context.op_override && !self.context.addr_override)
                            && valids[i].text.contains("32")
                        {
                            return InstructionResponse {
                                val: Some(valids[i].clone()),
                                size,
                            };
                        }
                    }
                    ArchSize::I64 => {
                        if (self.context.op_override || self.context.addr_override)
                            && valids[i].text.contains("16")
                        {
                            return InstructionResponse {
                                val: Some(valids[i].clone()),
                                size,
                            };
                        } else if (!self.context.op_override && !self.context.addr_override)
                            && (valids[i].text.contains("32") || valids[i].text.contains("64"))
                        {
                            return InstructionResponse {
                                val: Some(valids[i].clone()),
                                size,
                            };
                        }
                    }
                };
                i += 1;
            }
            // There are some jumps that are the same but have aliases
            // so just return the first one i guess
            return InstructionResponse {
                val: Some(valids[0].clone()),
                size,
            };
        }

        // What possibly can be here?
        // ??
        panic!("At the disco");

        // If instruction is invalid
        return InstructionResponse { val: None, size: 0 };
    }
}

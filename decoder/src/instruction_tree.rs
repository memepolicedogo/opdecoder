use core::panic;
use std::usize;
use std::{collections::HashMap, fmt};

use regex::Regex;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct InstructionJSON {
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

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Instruction {
    pub opcode: String,
    pub text: String,
    pub x64: bool,
    pub legacy: bool,
    pub operands: Option<Vec<Operand>>,
    pub size: OperandSize,
    pub invalid_prefixes: Vec<u8>,
    pub description: String,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, PartialOrd)]
pub enum OperandSize {
    Any = 0,
    Byte = 8,
    Word = 16,
    Double = 32,
    Quad = 64,
    DoubleSeg = 48,
    Penta = 80,
    DoubleQuad = 128,
    QuadQuad = 256,
    Z = 384,
    DoubleQuadQuad = 512, // man
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub enum OperandEncoding {
    Opcode,    // In instruction opcode
    Immediate, // Immediate value, including offsets
    Modrm,     // Modrm +? SIB byte(s)
    Modreg,
    Bespoke, // Something evil and vile
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, PartialOrd)]
pub enum RegisterType {
    GPReg,
    SegReg,
    FPUReg,
    MMXReg,
    BoundReg,
    KReg,
    CtrlReg,
    DbgReg,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Operand {
    pub size: OperandSize,         // Size of the value
    pub encoding: OperandEncoding, // How the value is encoded
    pub reg: Option<RegisterType>, // If it's a register, what kind
    pub text: String, // The actual text of the operand for edge cases where the previous data
                      // isn't enough
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
                return Some(*self.children.get(key).expect(""));
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

const VEX_THREE_BYTE_FORM_REQS: [&'static str; 5] = ["W0", "W1", "0F", "0F38", "0F3A"];

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
                        ')' => neg = !neg,
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

    pub fn from_legacy_json(json: &String) -> Vec<Vec<Instruction>> {
        let reg_mem_size_dif = Regex::new("r(8|16|32|64)/m(8|16|32|64)").unwrap();
        let mut tables: Vec<Vec<InstructionJSON>> = serde_json::from_str(json).expect("Bad JSON");
        let mut updated: Vec<Vec<Instruction>> = Vec::new();
        for table in tables {
            let mut instructions: Vec<Instruction> = Vec::new();
            for ins in table {
                // Convert to new instruction format
                let mut tmp = Instruction {
                    opcode: ins.opcode.clone(),
                    text: ins.text.clone(),
                    x64: ins.x64.starts_with("V"),
                    legacy: ins.legacy.starts_with("V"),
                    operands: Self::operands_from_instruction(&ins),
                    invalid_prefixes: if ins.opcode.contains("NP") {
                        vec![0xf2, 0xf3, 0x66]
                    } else if ins.opcode.contains("NFx") {
                        vec![0xf2, 0xf3]
                    } else {
                        Vec::new()
                    },
                    description: ins.description.clone(),
                    size: OperandSize::Any,
                };
                if tmp.operands.is_some() {
                    for o in tmp.operands.as_ref().unwrap() {
                        if o.size > tmp.size {
                            tmp.size = o.size.clone();
                        }
                    }
                }
                instructions.push(tmp);
            }
            updated.push(instructions);
        }
        return updated;
    }

    fn operands_from_instruction(instruction: &InstructionJSON) -> Option<Vec<Operand>> {
        let op_in_code = Regex::new("\\+[ir][bwdo]").unwrap();
        if instruction.operands.is_none() && !op_in_code.is_match(&instruction.opcode) {
            return None;
        }
        let ops = if instruction.operands.is_none() {
            // Evil FPU code
            &vec![String::from("opcode")]
        } else {
            instruction.operands.as_ref().unwrap()
        };
        if ops[0] == "N/A" {
            return None;
        }
        // ADD r/m64, imm8 -> ["ADD r/m64", " imm8"]
        let mut ins_ops: Vec<&str> = instruction.text.split(",").collect();
        // ["ADD r/m64", " imm8"] -> ["r/m64", " imm8"]
        if ins_ops.len() >= 1 && ins_ops[0].trim().contains(' ') {
            ins_ops[0] = ins_ops[0].split(" ").collect::<Vec<&str>>()[1];
        }
        let mut res = Vec::new();
        let mut i = 0;
        while i < ops.len() && ops[i] != "N/A" {
            // Evil bit shift endcoding
            if ops[i] == "1" {
                break;
            }
            ins_ops[i] = ins_ops[i].trim();
            let mut new = Operand {
                size: OperandSize::Any,
                encoding: OperandEncoding::Modrm,
                reg: None,
                text: String::from(ins_ops[i]),
            };
            // Get size
            new.size = if ins_ops[i].ends_with("512") {
                OperandSize::DoubleQuadQuad
            } else if ins_ops[i].ends_with("384") {
                OperandSize::Z
            } else if ins_ops[i].ends_with("256") {
                OperandSize::QuadQuad
            } else if ins_ops[i].ends_with("128") {
                OperandSize::DoubleQuad
            } else if ins_ops[i].ends_with("80") {
                OperandSize::Penta
            } else if ins_ops[i].ends_with("64") {
                OperandSize::Quad
            } else if ins_ops[i].ends_with("32") {
                OperandSize::Double
            } else if ins_ops[i].ends_with("16") {
                OperandSize::Word
            } else if ins_ops[i].ends_with("8") {
                OperandSize::Byte
            } else {
                OperandSize::Any
            };

            // Encoding
            new.encoding = if ops[i].contains("ModRM:reg") {
                OperandEncoding::Modreg
            } else if ops[i].contains("ModRM:r/m") {
                // Cases where seperate sizes are specified for register and memory moves
                if new.size <= OperandSize::Quad && !ins_ops[i].contains("r/m") {
                    OperandEncoding::Bespoke
                } else {
                    OperandEncoding::Modrm
                }
            } else if ops[i].starts_with("imm")
                || ops[i].starts_with("Offset")
                || ops[i].starts_with("Moffs")
            {
                OperandEncoding::Immediate
            } else if ops[i].contains("opcode") {
                OperandEncoding::Opcode
            } else {
                OperandEncoding::Bespoke
            };
            // Reg
            new.reg = match new.encoding {
                OperandEncoding::Modreg => {
                    if ins_ops[i].starts_with("CR") {
                        // Decode size as arch size at runtime
                        // CR0-7 will have this already but because CR8 ends with 8 previous code
                        // assumes it's byte width
                        new.size = OperandSize::Any;
                        Some(RegisterType::CtrlReg)
                    } else if ins_ops[i].starts_with("DR") {
                        new.size == OperandSize::Any;
                        Some(RegisterType::DbgReg)
                    } else if ins_ops[i].contains("mm") || new.size >= OperandSize::DoubleQuad {
                        Some(RegisterType::MMXReg)
                    } else if ins_ops[i] == "Sreg" {
                        new.size = OperandSize::Word;
                        Some(RegisterType::SegReg)
                    } else {
                        Some(RegisterType::GPReg)
                    }
                }
                OperandEncoding::Modrm => {
                    if ins_ops[i].contains("m16:") {
                        new.size = OperandSize::Word;
                        Some(RegisterType::SegReg)
                    } else {
                        Some(RegisterType::GPReg)
                    }
                }
                OperandEncoding::Bespoke => {
                    if ins_ops[i].contains("ST") {
                        new.size = OperandSize::Penta;
                        Some(RegisterType::FPUReg)
                    } else if ins_ops[i].contains("bnd") {
                        new.size = OperandSize::DoubleQuad;
                        Some(RegisterType::BoundReg)
                    } else if ins_ops[i].contains("k1") {
                        new.size = OperandSize::Quad;
                        Some(RegisterType::KReg)
                    } else {
                        None
                    }
                }
                OperandEncoding::Immediate => None,
                _ => Some(RegisterType::GPReg),
            };
            if new.encoding == OperandEncoding::Bespoke {
                // Handle special cases for bespokes
                // Unique BS in Enter
                if ops[i] == "iw" {
                    new.encoding = OperandEncoding::Immediate;
                    new.size = OperandSize::Word;
                } else if ins_ops[i].starts_with("ptr") {
                    new.encoding = OperandEncoding::Immediate;
                    new.size = match new.size {
                        OperandSize::Word => OperandSize::Double,
                        OperandSize::Double => OperandSize::DoubleSeg,
                        OperandSize::Quad => OperandSize::Penta,
                        _ => panic!("Invalid size for far pointer: {:?}", new.size),
                    };
                }
            }
            res.push(new);
            i += 1;
        }
        return Some(res);
    }

    pub fn from_json(json: &String) -> Self {
        let json_result = serde_json::from_str::<Vec<Vec<Instruction>>>(json);
        let tables = if json_result.is_err() {
            InstructionTree::from_legacy_json(json)
        } else {
            json_result.unwrap()
        };
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

#[derive(Debug, PartialEq)]
pub enum ArchSize {
    I16,
    I32,
    I64,
}

#[derive(Debug, Clone)]
pub struct Rex {
    pub w: bool,
    pub r: u8,
    pub b: u8,
    pub x: u8,
}
pub struct Modrm {
    pub mode: u8,
    pub rm: u8,
    pub reg: u8,
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

#[derive(Debug)]
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

impl Context {
    pub fn addr_size(&self) -> OperandSize {
        match self.size {
            ArchSize::I64 => {
                if self.addr_override {
                    OperandSize::Double
                } else {
                    OperandSize::Quad
                }
            }
            ArchSize::I32 => {
                if self.addr_override {
                    OperandSize::Word
                } else {
                    OperandSize::Double
                }
            }
            ArchSize::I16 => {
                if self.addr_override {
                    OperandSize::Byte
                } else {
                    OperandSize::Word
                }
            }
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
        let index = self.curr as isize + offset;
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
    pub pref_size: usize,
}

#[derive(Debug)]
pub struct OperandResponse {
    pub val: Option<Vec<String>>,
    pub size: usize,
}

#[derive(Debug, Serialize)]
pub struct ParseResponse {
    pub instruction: Option<Instruction>,
    pub operands: Option<Vec<String>>,
    pub bytes: Option<Vec<u8>>,
}

impl fmt::Display for ParseResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        return if self.bytes.is_none() {
            write!(f, "Failed to parse")
        } else if self.instruction.is_none() {
            write!(f, "{:02X}", self.bytes.as_ref().unwrap()[0])
        } else {
            let mut full_str = String::new();
            let ins = self.instruction.as_ref().unwrap();
            // Get the base instruction name sans ops
            full_str.push_str(ins.text.split(' ').collect::<Vec<_>>()[0]);
            if self.operands.is_some() {
                for op in self.operands.as_ref().unwrap() {
                    // Leading space and trailing comma for each
                    full_str.push(' ');
                    full_str.push_str(op);
                    full_str.push(',');
                }
                // Remove trailing comma
                full_str.pop();
            }
            write!(f, "{}", full_str)
        };
    }
}

impl ParseResponse {
    pub fn bytes_to_string(&self) -> String {
        return if self.bytes.is_none() {
            String::from("Failed to parse")
        } else {
            let mut str = String::new();
            for byte in self.bytes.as_ref().unwrap() {
                str += format!("{:02X} ", byte).as_str();
            }
            str
        };
    }
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

#[derive(Deserialize, Serialize)]
pub enum NumFormat {
    Hex,
    Dec,
    Bi,
    Oct,
}

#[derive(Deserialize, Serialize)]
#[serde(default)]
pub struct InstructionFormatting {
    // OPERAND
    pub reg_uppercase: bool,
    pub imm_uppercase: bool,
    pub addr_open: String,
    pub addr_close: String,
    pub addr_add: String,
    pub addr_mul: String,
    pub addr_scale_two: String,
    pub addr_scale_four: String,
    pub addr_scale_eight: String,
    pub addr_seg_seperator: String,
    pub addr_prefix: String,
    pub addr_byte: String,
    pub addr_word: String,
    pub addr_dword: String,
    pub addr_qword: String,
    pub addr_tword: String,
    pub addr_oword: String,
    pub addr_yword: String,
    pub addr_zword: String,
    pub imm_prefix: String,
    pub imm_suffix: String,
    pub imm_fmt: NumFormat,
    // INSTRUCTION
    pub ins_uppercase: bool,
    // CODE
    pub code_fmt: NumFormat,
}

impl Default for InstructionFormatting {
    fn default() -> Self {
        Self {
            reg_uppercase: true,
            imm_uppercase: true,
            addr_open: String::from("["),
            addr_close: String::from("]"),
            addr_add: String::from("+"),
            addr_mul: String::from("*"),
            addr_scale_two: String::from("2"),
            addr_scale_four: String::from("4"),
            addr_scale_eight: String::from("8"),
            addr_seg_seperator: String::from(":"),
            addr_prefix: String::from(""),
            addr_byte: String::from("byte "),
            addr_word: String::from("word "),
            addr_dword: String::from("dword "),
            addr_qword: String::from("qword "),
            addr_tword: String::from("tword "),
            addr_oword: String::from("oword "),
            addr_yword: String::from("yword "),
            addr_zword: String::from("zword "),
            imm_prefix: String::from("0x"),
            imm_suffix: String::from(""),
            imm_fmt: NumFormat::Hex,
            //
            ins_uppercase: true,
            //
            code_fmt: NumFormat::Hex,
        }
    }
}

pub struct Decoder {
    pub context: Context,
    pub format: InstructionFormatting,
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
    pub fn has_code(&self) -> bool {
        !self.code.code.is_empty()
    }

    pub fn load_code(&mut self, code: &Vec<u8>) {
        self.code = ByteString {
            code: code.clone(),
            curr: 0,
        };
    }

    pub fn append_code(&mut self, code: &Vec<u8>) {
        self.code.append(code);
    }

    pub fn parse_n_print(&mut self) {
        while !self.code.is_end() {
            let inc = self.parse_one();
            inc.pretty_print();
        }
    }

    //
    pub fn parse(&mut self) -> Vec<ParseResponse> {
        let mut responses = Vec::new();
        while !self.code.is_end() {
            let rep = self.parse_one();
            responses.push(rep);
        }
        responses
    }

    pub fn parse_n(&mut self, n: usize) -> Vec<ParseResponse> {
        let mut responses = Vec::new();
        while !self.code.is_end() && responses.len() < n {
            let rep = self.parse_one();
            responses.push(rep);
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
        // Format instruction
        let operands = self.parse_operands(&instruction);
        let start_offset = -((instruction.size + instruction.pref_size + operands.size) as isize);
        ParseResponse {
            instruction: instruction.val,
            operands: operands.val,
            bytes: Some(Vec::from(self.code.get_slice_offset(start_offset, 0))),
        }
    }

    fn parse_modrm(
        &mut self,
        reg: &RegisterType,
        size: &OperandSize,
        modrm: &Modrm,
        rex: &Rex,
        ins_size: &mut usize,
    ) -> String {
        let mut res = String::new();
        // Prepend a square bracket for effective address format, mod = 0b11 reassigns
        // res so this isn't present there
        res.push_str(&self.format.addr_open);
        if modrm.mode == 0b11 {
            // Explicit register
            res = self.format_reg((modrm.rm | rex.b) as usize, size, reg);
        } else if modrm.mode == 0 && modrm.rm == 0b101 {
            // Special case: immidiate offset
            if rex.w {
                res += &self.format_imm(8);
                *ins_size += 8;
            } else if self.context.rex.is_none() && self.context.op_override {
                res += &self.format_imm(2);
                *ins_size += 2;
            } else {
                res += &self.format_imm(4);
                *ins_size += 4;
            };
        } else {
            if modrm.rm == 0b100 {
                //SIB
                let sib = self.code.get();
                self.code.inc();
                *ins_size += 1;
                let scale = (sib & 0b11000000) >> 6;
                let index = (sib & 0b00111000) >> 3;
                let base = sib & 0b00000111;
                // Index
                if (rex.x | index) == 0b100 {
                } else {
                    res += &self.format_reg(
                        (index | rex.x) as usize,
                        &(self.context.addr_size()),
                        reg,
                    );
                    match scale {
                        1 => {
                            res.push_str(&self.format.addr_mul);
                            res.push_str(&self.format.addr_scale_two);
                            res.push_str(&self.format.addr_add);
                        }
                        2 => {
                            res.push_str(&self.format.addr_mul);
                            res.push_str(&self.format.addr_scale_four);
                            res.push_str(&self.format.addr_add);
                        }
                        3 => {
                            res.push_str(&self.format.addr_mul);
                            res.push_str(&self.format.addr_scale_eight);
                            res.push_str(&self.format.addr_add);
                        }
                        _ => res.push_str(&self.format.addr_add),
                    }
                }
                // Base
                if base != 0b101 {
                    res +=
                        &self.format_reg((base | rex.b) as usize, &(self.context.addr_size()), reg);
                } else {
                    // When base is 0b101 it means either it's based on RBP or a
                    // displacement, depending on mod
                    // Base reg based on arch size and prefixes
                    let basereg = self.format_reg(5, &self.context.addr_size(), reg);
                    match modrm.mode {
                        // Just displacement
                        0 => {
                            if rex.w {
                                res.push_str(&self.format_imm(4));
                                *ins_size += 4;
                            } else {
                                res.push_str(&self.format_imm(4));
                                *ins_size += 4;
                            }
                        }
                        // disp8 + ebp
                        1 => {
                            res.push_str(&self.format_imm(1));
                            *ins_size += 1;
                            res.push_str(&self.format.addr_add);
                            if self.format.reg_uppercase {
                                res.push_str(&basereg);
                            } else {
                                res.push_str(&(basereg.to_lowercase()));
                            }
                        }
                        // disp32 + ebp
                        // all this to enable C local variabes. Very cool
                        2 => {
                            res.push_str(&self.format_imm(4));
                            *ins_size += 4;
                            res.push_str(&self.format.addr_add);
                            if self.format.reg_uppercase {
                                res.push_str(&basereg);
                            } else {
                                res.push_str(&(basereg.to_lowercase()));
                            }
                        }
                        _ => {}
                    }
                }
            } else {
                // Normal base reg
                res += &self.format_reg(
                    (modrm.rm | rex.b) as usize,
                    &(self.context.addr_size()),
                    reg,
                );
            }
            match modrm.mode {
                0b1 => {
                    res.push_str(&self.format.addr_add);
                    *ins_size += 1;
                    res += &self.format_imm(1);
                }
                0b10 => {
                    res.push_str(&self.format.addr_add);
                    *ins_size += 4;
                    res += &self.format_imm(4);
                }
                _ => {}
            }
        }
        if res.starts_with(&self.format.addr_open) {
            res.push_str(&self.format.addr_close);
            // Add prefixes
            match size {
                &OperandSize::Byte => {
                    res.insert_str(0, &self.format.addr_byte);
                }
                &OperandSize::Word => {
                    res.insert_str(0, &self.format.addr_word);
                }
                &OperandSize::Double => {
                    res.insert_str(0, &self.format.addr_dword);
                }
                &OperandSize::Quad => {
                    res.insert_str(0, &self.format.addr_qword);
                }
                &OperandSize::Penta => {
                    res.insert_str(0, &self.format.addr_tword);
                }
                &OperandSize::DoubleQuad => {
                    res.insert_str(0, &self.format.addr_oword);
                }
                &OperandSize::QuadQuad => {
                    res.insert_str(0, &self.format.addr_yword);
                }
                &OperandSize::DoubleQuadQuad => {
                    res.insert_str(0, &self.format.addr_zword);
                }
                _ => {}
            }
            res.insert_str(0, &self.format.addr_prefix);
        }
        res
    }

    fn parse_operands(&mut self, ins: &InstructionResponse) -> OperandResponse {
        let instruction = ins.val.as_ref().unwrap();
        if instruction.operands.is_none() {
            return OperandResponse {
                ..Default::default()
            };
        }
        let rex = if self.context.rex.is_some() {
            self.context.rex.as_ref().unwrap().clone()
        } else {
            Rex {
                w: false,
                r: 0,
                b: 0,
                x: 0,
            }
        };
        // # of operands
        let mut offset = 0;
        // # of bytes comprising the opperands
        let mut size = 0;
        let mut op_strings = Vec::new();
        // We have to store this ahead of time because it encodes two values, modrm and modreg, and
        // the order of these operands isn't consistant, so modreg may be parsed before or after
        // modrm, which may advance the decoding to parse an SIB byte, ergo we can't rely on the
        // current code to be the modrm byte
        let mut modrm = Modrm {
            mode: (self.code.get() & 0b11000000) >> 6,
            reg: (self.code.get() & 0b00111000) >> 3,
            rm: (self.code.get() & 0b00000111),
        };
        let mut has_modrm = false;
        for op in instruction.operands.as_ref().unwrap() {
            // Consider prefixes for ops of unspecified size
            let mut real_size = if op.size != OperandSize::Any {
                &op.size
            } else if rex.w {
                &OperandSize::Quad
            } else if self.context.rex.is_none() && self.context.op_override {
                &OperandSize::Word
            } else {
                &OperandSize::Double
            };
            let mut op_str = String::new();
            match op.encoding {
                OperandEncoding::Modrm => {
                    let real_reg = if op.reg == Some(RegisterType::MMXReg) {
                        &RegisterType::MMXReg
                    } else {
                        &RegisterType::GPReg
                    };
                    // To prevent modRM double count
                    if !has_modrm {
                        size += 1;
                        has_modrm = true;
                        // Advance to potential SIB byte
                        self.code.inc();
                    }
                    op_str = self.parse_modrm(real_reg, real_size, &modrm, &rex, &mut size);
                }
                OperandEncoding::Modreg => {
                    // To prevent modRM double count
                    if !has_modrm {
                        size += 1;
                        has_modrm = true;
                        // Advance to potential SIB byte
                        self.code.inc();
                    }
                    op_str = self.format_reg(
                        (modrm.reg | rex.r) as usize,
                        real_size,
                        &op.reg.as_ref().unwrap_or(&RegisterType::GPReg),
                    );
                }
                OperandEncoding::Opcode => {
                    op_str = self.format_reg(
                        // Get last byte of opcode, logical and to get last 3 bits, include REX
                        // prefix, cast to usize for type jit
                        ((self.code.get_offset(-((size + 1) as isize)) & 0b00000111) | rex.b)
                            as usize,
                        real_size,
                        &op.reg.as_ref().unwrap_or(&RegisterType::GPReg),
                    );
                }
                OperandEncoding::Immediate => {
                    op_str = match real_size {
                        OperandSize::Byte => {
                            size += 1;
                            self.format_imm(1)
                        }
                        OperandSize::Word => {
                            size += 2;
                            self.format_imm(2)
                        }
                        OperandSize::Double => {
                            size += 4;
                            self.format_imm(4)
                        }
                        OperandSize::DoubleSeg => {
                            size += 6;
                            self.format_imm(6)
                        }
                        OperandSize::Quad => {
                            size += 8;
                            self.format_imm(8)
                        }
                        OperandSize::Penta => {
                            size += 10;
                            self.format_imm(10)
                        }
                        OperandSize::DoubleQuad => {
                            size += 16;
                            self.format_imm(16)
                        }
                        OperandSize::QuadQuad => {
                            size += 32;
                            self.format_imm(32)
                        }
                        OperandSize::Z => {
                            size += 48;
                            self.format_imm(48)
                        }
                        OperandSize::DoubleQuadQuad => {
                            size += 64;
                            self.format_imm(64)
                        }
                        OperandSize::Any => {
                            panic!("Immediate value size cannot be infered");
                        }
                    };
                }
                OperandEncoding::Bespoke => {
                    let is_reg = Regex::new("([ER]?[AC]X)|([AC][HL])").unwrap();
                    let reg_mem_size_dif = Regex::new("r(8|16|32|64)/m(8|16|32|64)").unwrap();
                    // If register literal
                    if is_reg.is_match(&op.text) {
                        op_str = op.text.clone();
                        if !self.format.reg_uppercase {
                            op_str = op_str.to_lowercase();
                        }
                    } else if op.text == "mib" {
                        // Some sort of evil subset of SIB addressing
                        let base = self.code.get() & 0b111;
                        self.code.inc();
                        size += 1;
                        if base == 0b101 {
                            // Displacment
                            let basereg = if self.context.size == ArchSize::I64 {
                                "RBP"
                            } else {
                                "EBP"
                            };
                            match modrm.mode {
                                // Just displacement
                                0 => {
                                    if rex.w {
                                        op_str.push_str(&self.format_imm(4));
                                        size += 4;
                                    } else {
                                        op_str.push_str(&self.format_imm(4));
                                        size += 4;
                                    }
                                }
                                // disp8 + ebp
                                1 => {
                                    op_str.push_str(&self.format_imm(1));
                                    size += 1;
                                    op_str.push_str(&self.format.addr_add);
                                    if self.format.reg_uppercase {
                                        op_str.push_str(&basereg);
                                    } else {
                                        op_str.push_str(&(basereg.to_lowercase()));
                                    }
                                }
                                // disp32 + ebp
                                // all this to enable C local variabes. Very cool
                                2 => {
                                    op_str.push_str(&self.format_imm(4));
                                    size += 4;
                                    op_str.push_str(&self.format.addr_add);
                                    if self.format.reg_uppercase {
                                        op_str.push_str(&basereg);
                                    } else {
                                        op_str.push_str(&(basereg.to_lowercase()));
                                    }
                                }
                                _ => {}
                            }
                        } else {
                            op_str.push_str(&self.format.addr_open);
                            op_str += &self.format_reg(
                                (base | rex.b) as usize,
                                &(self.context.addr_size()),
                                &RegisterType::GPReg,
                            );
                            op_str.push_str(&self.format.addr_close);
                        }
                    } else if reg_mem_size_dif.is_match(&op.text) {
                        // Missmatched sizes for register vs memory access
                        // Treat as normal modrm mostly
                        let real_reg = if op.reg == Some(RegisterType::MMXReg) {
                            &RegisterType::MMXReg
                        } else {
                            &RegisterType::GPReg
                        };
                        // To prevent modRM double count
                        if !has_modrm {
                            size += 1;
                            has_modrm = true;
                            // Advance to potential SIB byte
                            self.code.inc();
                        }
                        if modrm.mode == 0b11 {
                            // Register literal, context based size
                            op_str = self.parse_modrm(
                                real_reg,
                                if rex.w {
                                    &OperandSize::Quad
                                } else if self.context.rex.is_none() && self.context.op_override {
                                    &OperandSize::Word
                                } else {
                                    &OperandSize::Double
                                },
                                &modrm,
                                &rex,
                                &mut size,
                            );
                        } else {
                            // Memory, sized according to op
                            op_str = self.parse_modrm(real_reg, &op.size, &modrm, &rex, &mut size);
                        }
                    } else if op.text.contains("16:") {
                        // Far pointer
                        let real_reg = if op.reg == Some(RegisterType::MMXReg) {
                            &RegisterType::MMXReg
                        } else {
                            &RegisterType::GPReg
                        };
                        // To prevent modRM double count
                        if !has_modrm {
                            size += 1;
                            has_modrm = true;
                            // Advance to potential SIB byte
                            self.code.inc();
                        }
                        op_str = self.parse_modrm(real_reg, real_size, &modrm, &rex, &mut size);
                        op_str.insert_str(
                            self.format.addr_prefix.len(),
                            &self.format.addr_seg_seperator,
                        );
                        op_str.insert_str(
                            self.format.addr_prefix.len(),
                            self.format.addr_word.trim(),
                        );
                    } else if op.text == "m" {
                        // LEA
                        let real_reg = if op.reg == Some(RegisterType::MMXReg) {
                            &RegisterType::MMXReg
                        } else {
                            &RegisterType::GPReg
                        };
                        // To prevent modRM double count
                        if !has_modrm {
                            size += 1;
                            has_modrm = true;
                            // Advance to potential SIB byte
                            self.code.inc();
                        }
                        // TODO: improve LEA formatting
                        op_str = self.parse_modrm(real_reg, real_size, &modrm, &rex, &mut size);
                    } else {
                        println!("{:#?}", instruction);
                        panic!("Unknown bespoke");
                    }
                }
            }
            op_strings.push(op_str);
        }
        if op_strings.is_empty() {
            OperandResponse {
                ..Default::default()
            }
        } else {
            OperandResponse {
                val: Some(op_strings),
                size,
            }
        }
    }

    fn format_imm(&mut self, count: usize) -> String {
        let mut i = 0;
        let mut val: u64 = 0;
        while i < count {
            val += (self.code.get() as u64) << (i * 8);
            self.code.inc();
            i += 1;
        }
        let mut out = match self.format.imm_fmt {
            NumFormat::Hex => {
                format!("{:02X}", val)
            }
            NumFormat::Dec => {
                format!("{}", val)
            }
            NumFormat::Oct => {
                format!("{:o}", val)
            }
            NumFormat::Bi => {
                format!("{:08b}", val)
            }
        };
        if !self.format.imm_uppercase {
            out = out.to_lowercase();
        }
        out.insert_str(0, &self.format.imm_prefix);
        out.push_str(&self.format.imm_suffix);
        out
    }

    fn format_reg(&self, index: usize, size: &OperandSize, group: &RegisterType) -> String {
        let mut result;
        match group {
            RegisterType::GPReg => {
                result = if self.context.rex.is_some() || *size != OperandSize::Byte {
                    String::from(BASE_REGS_REX_EXTENDED[index])
                } else {
                    // These should only be used for byte operations
                    String::from(BASE_REGS[index])
                };

                match size {
                    OperandSize::Quad => {
                        if result.starts_with("R") {
                        } else if result.len() == 1 {
                            result.insert(0, 'R');
                            result = result + "X";
                        } else {
                            result.insert(0, 'R');
                        }
                    }
                    OperandSize::Double => {
                        if result.starts_with("R") {
                            result = result + "D";
                        } else if result.len() == 1 {
                            result.insert(0, 'E');
                            result = result + "X";
                        } else {
                            result.insert(0, 'E');
                        }
                    }
                    OperandSize::Word => {
                        if result.starts_with("R") {
                            result = result + "W";
                        } else if result.len() == 1 {
                            result = result + "X";
                        }
                    }
                    OperandSize::Byte => {
                        if result.starts_with("R") {
                            result = result + "B";
                        }
                    }
                    _ => panic!("Invalid operand size for General Purpose Register"),
                }
            }

            RegisterType::MMXReg => {
                result = String::from("MM");
                match size {
                    // ZMM
                    OperandSize::DoubleQuadQuad => {
                        // Size prefix
                        result.insert(0, 'Z');
                        // Append number to end
                        result += &index.to_string();
                    }
                    // YMM
                    OperandSize::QuadQuad => {
                        // Size prefix
                        result.insert(0, 'Y');
                        // Append number to end
                        result += &index.to_string();
                    }
                    // XMM
                    OperandSize::DoubleQuad => {
                        // Size prefix
                        result.insert(0, 'X');
                        // Append number to end
                        result += &index.to_string();
                    }
                    // MM
                    OperandSize::Quad => {
                        // Append number to end
                        result += &index.to_string();
                    }
                    _ => panic!("Invalid operand size for MMX Register"),
                }
            }

            RegisterType::KReg => {
                result = String::from('K');
                result += &index.to_string();
            }

            RegisterType::BoundReg => {
                result = String::from("BND");
                if index < 4 {
                    result += &index.to_string();
                } else {
                    panic!("Invalid register index for bounds register");
                }
            }

            RegisterType::SegReg => {
                result = String::from(match index {
                    0 => "ES",
                    1 => "CS",
                    2 => "SS",
                    3 => "DS",
                    4 => "FS",
                    5 => "GS",
                    _ => panic!("Invalid register index for segment register"),
                });
            }

            RegisterType::FPUReg => {
                result = String::from("ST(");
                result += &index.to_string();
                result.push(')');
            }

            RegisterType::CtrlReg => {
                result = String::from("CR");
                // CR8 is only accessable when REX.R is set
                if self.context.rex.is_some() && self.context.rex.as_ref().unwrap().r == 1 {
                    result += "8"
                } else {
                    result += &index.to_string();
                }
            }

            RegisterType::DbgReg => {
                result = String::from("DR");
                result += &index.to_string();
            }
        }
        if !self.format.reg_uppercase {
            result = result.to_lowercase();
        }
        result
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
            for i in prefix_count..MAX_WIDTH {
                byte = self.code.get_offset(i as isize);
                let rep = self.tree.step(byte);
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
                    size = (i + 1) - prefix_count;
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
            return InstructionResponse {
                val: None,
                size: 1,
                pref_size: 0,
            };
        }
        // Figure out the prefixes
        for byte in prefix {
            // If byte isn't in range to be a valid prefix then escape
            if (byte & 0b11110000) == 0b01000000 && self.context.size == ArchSize::I64 {
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
        while i < valids.len() && valids.len() > 1 {
            match self.context.size {
                ArchSize::I16 => {
                    if !valids[i].legacy || valids[i].size > OperandSize::Word {
                        valids.remove(i);
                    } else {
                        i += 1;
                    }
                }
                ArchSize::I32 => {
                    if !valids[i].legacy || valids[i].size == OperandSize::Quad {
                        valids.remove(i);
                    } else {
                        i += 1;
                    }
                }
                ArchSize::I64 => {
                    // Invalid for target arch
                    if !valids[i].x64 {
                        valids.remove(i);
                    } else if valids[i].size == OperandSize::Double
                        && (self.context.addr_override || self.context.op_override)
                    {
                        // If there is an override prefix then it can't be a 32 bit instruction
                        valids.remove(i);
                    } else if valids[i].size < OperandSize::Quad
                        && !(self.context.addr_override || self.context.op_override)
                    {
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
                size: 0,
                pref_size: 0,
            };
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
            let mut rep = valids[0].clone();
            if !self.format.ins_uppercase {
                rep.text = rep.text.to_lowercase();
            }
            return InstructionResponse {
                val: Some(rep),
                size,
                pref_size: prefix_count,
            };
        } else if valids[0].description.starts_with("Jump")
            || valids[0].description.starts_with("Mult")
            || valids[0].description.starts_with("Exchange")
            || valids[0].text.starts_with("CMOV")
        {
            // Jump instructions have multiple identical entries where the logical operation is
            // the same but can be refered to in differnt ways, i.e. JL == JNGE
            let mut rep = valids[0].clone();
            if !self.format.ins_uppercase {
                rep.text = rep.text.to_lowercase();
            }
            return InstructionResponse {
                val: Some(rep),
                size,
                pref_size: prefix_count,
            };
        } else {
            // Between size specified and unspecified we prefer unspecified because it's the same
            // thing only dependant on prefixes or whatever
            let reg_mem_size_dif = Regex::new("r(8|16|32|64)/m(8|16|32|64)").unwrap();
            for i in 0..valids.len() {
                if reg_mem_size_dif.is_match(&valids[i].text) {
                    let mut rep = valids[i].clone();
                    if !self.format.ins_uppercase {
                        rep.text = rep.text.to_lowercase();
                    }
                    return InstructionResponse {
                        val: Some(rep),
                        size,
                        pref_size: prefix_count,
                    };
                }
            }
        }

        // What possibly can be here?
        // ??
        println!("{:#?}", valids);
        println!("Context: {:#?}", self.context);
        panic!("Multiple instructions found matching paramaters");
    }
}

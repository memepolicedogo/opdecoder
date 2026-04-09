use crate::decoder::{CustomFormat, InstructionFormatting};
use core::panic;
use std::{collections::HashMap, fmt};

use bevy_reflect::Reflect;
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

#[derive(Deserialize, Serialize, Debug, Clone, Reflect)]
pub struct Instruction {
    pub opcode: String,
    pub text: String,
    pub x64: bool,
    pub legacy: bool,
    pub operands: Option<Vec<GenericOperand>>,
    pub size: OperandSize,
    pub invalid_prefixes: Vec<u8>,
    pub description: String,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, PartialOrd, Reflect, Copy)]
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

impl CustomFormat for OperandSize {
    fn custom_format(&self, opts: &InstructionFormatting) -> String {
        match self {
            OperandSize::Any => String::new(),
            OperandSize::Byte => opts.addr_byte.clone(),
            OperandSize::Word => opts.addr_word.clone(),
            OperandSize::Double => opts.addr_dword.clone(),
            OperandSize::Quad => opts.addr_qword.clone(),
            OperandSize::DoubleSeg => {
                let mut res = opts.addr_word.clone();
                res += &opts.addr_seg_seperator;
                res += &opts.addr_dword;
                res
            }
            OperandSize::Penta => opts.addr_tword.clone(),
            OperandSize::DoubleQuad => opts.addr_oword.clone(),
            OperandSize::QuadQuad => opts.addr_yword.clone(),
            OperandSize::Z => opts.addr_zword.clone(),
            OperandSize::DoubleQuadQuad => String::new(),
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Reflect)]
pub enum OperandEncoding {
    Opcode,    // In instruction opcode
    Immediate, // Immediate value, including offsets
    Modrm,     // Modrm +? SIB byte(s)
    Modreg,
    Bespoke, // Something evil and vile
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, PartialOrd, Reflect)]
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

#[derive(Deserialize, Serialize, Debug, Clone, Reflect)]
pub struct GenericOperand {
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
        let tables: Vec<Vec<InstructionJSON>> = serde_json::from_str(json).expect("Bad JSON");
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

    fn operands_from_instruction(instruction: &InstructionJSON) -> Option<Vec<GenericOperand>> {
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
            if ops[i] == "1" || ops[i].starts_with("Implicit") {
                break;
            }
            ins_ops[i] = ins_ops[i].trim();
            let mut new = GenericOperand {
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
            } else if ins_ops[i].ends_with("128") || ins_ops[i].starts_with("xmm") {
                OperandSize::DoubleQuad
            } else if ins_ops[i].ends_with("80") {
                OperandSize::Penta
            } else if ins_ops[i].ends_with("64") || ins_ops[i].starts_with("mm") {
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
                if new.size <= OperandSize::Quad
                    && ins_ops[i].contains('m')
                    && ins_ops[i].contains('r')
                    && !(ins_ops[i].contains("r/m")
                        || ins_ops[i].contains("16:32")
                        || ins_ops[i].starts_with("mm"))
                {
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
                        new.size = OperandSize::Any;
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
                    } else if ins_ops[i].starts_with("CR") {
                        // Decode size as arch size at runtime
                        // CR0-7 will have this already but because CR8 ends with 8 previous code
                        // assumes it's byte width
                        new.size = OperandSize::Any;
                        Some(RegisterType::CtrlReg)
                    } else if ins_ops[i].starts_with("DR") {
                        new.size = OperandSize::Any;
                        Some(RegisterType::DbgReg)
                    } else if ins_ops[i].contains("mm") || new.size >= OperandSize::DoubleQuad {
                        Some(RegisterType::MMXReg)
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
                    } else if ins_ops[i].ends_with("32&32") {
                        new.size = OperandSize::Quad;
                        new.encoding = OperandEncoding::Modrm;
                        None
                    } else if ins_ops[i].ends_with("16&16") {
                        new.size = OperandSize::Double;
                        new.encoding = OperandEncoding::Modrm;
                        None
                    } else if ins_ops[i].ends_with("16&64") {
                        new.size = OperandSize::Penta;
                        new.encoding = OperandEncoding::Modrm;
                        None
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

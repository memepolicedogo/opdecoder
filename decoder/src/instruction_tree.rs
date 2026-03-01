use core::panic;
use std::{collections::HashMap, fmt};

use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Instruction {
    pub opcode: String,
    #[serde(rename = "instruction")]
    pub text: String,
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

struct ParseResponse<'a> {
    val: Vec<&'a Instruction>,
    offset: usize,
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
}

enum ArchSize {
    I16,
    I32,
    I64,
}

struct Context {
    size: ArchSize,
    one: u8,
    two: u8,
    op_override: bool,
    addr_override: bool,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            size: ArchSize::I32,
            one: 0,
            two: 0,
            op_override: false,
            addr_override: false,
        }
    }
}

struct Decoder {
    context: Context,
    tree: InstructionTree,
}

const MAX_PREFIX: usize = 4;
const MAX_WIDTH: usize = MAX_PREFIX + 8;

impl Decoder {
    pub fn parse(&mut self, bytestring: &Vec<u8>) -> ParseResponse<'_> {
        let mut offset: usize = 0;
        let mut byte: u8;
        let mut prefix = Vec::new();
        let mut opcode = Vec::new();
        // Reset Context
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
            return ParseResponse {
                val: Vec::new(),
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

            offset += 1;
        }
        // Context is probably accurate now idk
        // Now we have to do conflict resolution and ensure that the prefixes and the instruction
        // match
        for instruction in ins {
            // Check if instruction matches the vibes
        }

        // If instruction is invalid
        return ParseResponse {
            val: Vec::new(),
            offset: 1,
        };
    }
}

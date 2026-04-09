use core::panic;
use std::collections::HashMap;

use crate::instruction_tree::{
    Instruction, InstructionTree, OperandEncoding, OperandSize, RegisterType,
};
use bevy_reflect::Reflect;
use regex::Regex;
use serde::{Deserialize, Serialize};

pub trait CustomFormat {
    fn custom_format(&self, opts: &InstructionFormatting) -> String;
}

#[derive(Deserialize, Serialize, Debug, Clone, Reflect)]
pub enum SIBScale {
    Zero,
    Double,
    Quad,
    Octo,
}

#[derive(Deserialize, Serialize, Debug, Clone, Reflect)]
pub enum Operand {
    Reg(Register),
    Imm(Immediate),
    Addr(Address),
    Bes(Bespoke),
}

impl CustomFormat for Operand {
    fn custom_format(&self, opts: &InstructionFormatting) -> String {
        match self {
            Operand::Reg(reg) => reg.custom_format(opts),
            Operand::Imm(imm) => imm.custom_format(opts),
            Operand::Addr(addr) => addr.custom_format(opts),
            Operand::Bes(bes) => bes.custom_format(opts),
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Reflect)]
pub struct Bespoke {
    pub value: String,
}

impl CustomFormat for Bespoke {
    fn custom_format(&self, _opts: &InstructionFormatting) -> String {
        self.value.clone()
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Reflect)]
pub struct Register {
    pub index: usize,
    pub size: OperandSize,
    pub group: RegisterType,
    pub rex: bool,
}

impl CustomFormat for Register {
    fn custom_format(&self, opts: &InstructionFormatting) -> String {
        opts.format_reg(self.index, &self.size, &self.group, self.rex)
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Reflect)]
pub struct Immediate {
    pub value: u64,
}

impl CustomFormat for Immediate {
    fn custom_format(&self, opts: &InstructionFormatting) -> String {
        let mut res = String::from(&opts.imm_prefix);
        res += &match opts.imm_fmt {
            NumFormat::Bi => {
                format!("{:b}", self.value)
            }
            NumFormat::Oct => {
                format!("{:o}", self.value)
            }
            NumFormat::Dec => {
                format!("{}", self.value)
            }
            NumFormat::Hex => {
                format!("{:x}", self.value)
            }
        };
        res.push_str(&opts.imm_suffix);
        res
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Reflect)]
pub struct Address {
    pub dest_size: OperandSize,
    pub addr_size: OperandSize,
    pub base: usize, // the REX.B + r/m (without sib) or REX.B + base (with sib)
    pub index: Option<usize>, // REX.X + index with SIB or None without
    pub scale: SIBScale, // Based on SS in SIB, only considered if index is set
    pub rm_disp: Option<Immediate>, // Displacement specified by mod bits of modrm
    pub sib_disp: Option<Immediate>, // Displacement specified by mod + base bits of SIB
    pub no_base: bool,
}

impl CustomFormat for Address {
    fn custom_format(&self, opts: &InstructionFormatting) -> String {
        let mut out = self.dest_size.custom_format(opts);
        out.insert_str(0, &opts.addr_prefix);
        out.push('[');
        // Index & Scale
        match self.index {
            Some(index) => {
                out.push_str(&opts.format_reg(index, &self.addr_size, &RegisterType::GPReg, true));
                match self.scale {
                    SIBScale::Double => {
                        out.push_str(&opts.addr_mul);
                        out.push_str(&opts.addr_scale_two);
                    }
                    SIBScale::Quad => {
                        out.push_str(&opts.addr_mul);
                        out.push_str(&opts.addr_scale_four);
                    }
                    SIBScale::Octo => {
                        out.push_str(&opts.addr_mul);
                        out.push_str(&opts.addr_scale_eight);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        // Base
        // Check if other elements have been added
        if !out.ends_with('[') {
            out.push_str(&opts.addr_add);
        }
        // One encoding only uses a displacement
        if !self.no_base {
            out.push_str(&opts.format_reg(self.base, &self.addr_size, &RegisterType::GPReg, true));
        }
        // Disps
        match &self.rm_disp {
            Some(disp) => {
                if !out.ends_with('+') && !out.ends_with('[') {
                    out.push_str(&opts.addr_add);
                }
                out.push_str(&disp.custom_format(opts));
            }
            _ => {}
        }
        match &self.sib_disp {
            Some(disp) => {
                if !out.ends_with('+') {
                    out.push_str(&opts.addr_add);
                }
                out.push_str(&disp.custom_format(opts));
            }
            _ => {}
        }
        if out.ends_with('+') {
            out.pop();
        }
        out.push(']');
        out
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
    pub fn len(&self) -> usize {
        self.code.len()
    }
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
        if self.is_end() {
            return self.code[self.curr];
        }
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
        self.code.len() == 0 || self.curr >= (self.code.len() - 1)
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
    pub prefixes: Option<Vec<u8>>,
}

#[derive(Debug)]
pub struct OperandResponse {
    pub val: Option<Vec<Operand>>,
    pub size: usize,
}

#[derive(Debug, Serialize, Reflect, Clone)]
pub struct ParseResponse {
    pub instruction: Option<Instruction>,
    pub operands: Option<Vec<Operand>>,
    pub bytes: Option<Vec<u8>>,
    pub prefixes: Option<Vec<u8>>,
}

impl CustomFormat for ParseResponse {
    fn custom_format(&self, opts: &InstructionFormatting) -> String {
        if self.bytes.is_none() {
            format!("Failed to parse")
        } else if self.instruction.is_none() {
            format!("{:02X}", self.bytes.as_ref().unwrap()[0])
        } else {
            let mut full_str = String::new();
            if self.prefixes.is_some() {
                for prefix in self.prefixes.as_ref().unwrap() {
                    let str = opts.prefixes.get(prefix);
                    if str.is_some() {
                        full_str.push_str(str.unwrap());
                    }
                }
            }
            let ins = self.instruction.as_ref().unwrap();
            // Get the base instruction name sans ops
            full_str.push_str(ins.text.split(' ').collect::<Vec<_>>()[0]);
            if self.operands.is_some() {
                for op in self.operands.as_ref().unwrap() {
                    // Leading space and trailing comma for each
                    full_str.push(' ');
                    full_str.push_str(&op.custom_format(opts));
                    full_str.push(',');
                }
                // Remove trailing comma
                full_str.pop();
            }
            full_str
        }
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

#[derive(Deserialize, Serialize, Reflect, Clone)]
pub enum NumFormat {
    Hex = 16,
    Dec = 10,
    Bi = 2,
    Oct = 8,
}

#[derive(Deserialize, Serialize, Reflect, Clone)]
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
    pub prefixes: HashMap<u8, String>,
    // CODE
    pub code_fmt: NumFormat,
    // REGISTERS
    pub mm_base: String,
    pub k_base: String,
    pub bnd_base: String,
    pub fpu_base: String,
    pub ctrl_base: String,
    pub dbg_base: String,
    pub rex_gp_set: Vec<String>,
    pub gp_set: Vec<String>,
    pub seg_set: Vec<String>,
    // NON-CODE
    pub comment: String,
    pub section: String,
}

impl Default for InstructionFormatting {
    fn default() -> Self {
        let mut prefixes: HashMap<u8, String> = HashMap::new();
        prefixes.insert(0xF0, String::from("LOCK "));
        prefixes.insert(0xF2, String::from("REPNE "));
        prefixes.insert(0xF3, String::from("REP "));

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
            prefixes,
            //
            code_fmt: NumFormat::Hex,
            //
            mm_base: String::from("MM"),
            k_base: String::from("K"),
            bnd_base: String::from("BND"),
            fpu_base: String::from("ST"),
            ctrl_base: String::from("CR"),
            dbg_base: String::from("DR"),
            gp_set: vec![
                String::from("A"),
                String::from("C"),
                String::from("D"),
                String::from("B"),
                String::from("AH"),
                String::from("BH"),
                String::from("CH"),
                String::from("DH"),
            ],
            rex_gp_set: vec![
                String::from("A"),
                String::from("C"),
                String::from("D"),
                String::from("B"),
                String::from("SP"),
                String::from("BP"),
                String::from("SI"),
                String::from("DI"),
                String::from("R8"),
                String::from("R9"),
                String::from("R10"),
                String::from("R11"),
                String::from("R12"),
                String::from("R13"),
                String::from("R14"),
                String::from("R15"),
            ],
            seg_set: vec![
                String::from("ES"),
                String::from("CS"),
                String::from("SS"),
                String::from("DS"),
                String::from("FS"),
                String::from("GS"),
            ],
            //
            comment: String::from(";"),
            section: String::from("section "),
        }
    }
}

impl InstructionFormatting {
    pub fn as_comment(&self, text: &String) -> String {
        return self.comment.clone() + text;
    }
    pub fn as_section(&self, text: &String) -> String {
        return self.section.clone() + text;
    }

    fn format_prefixes(&self, prefixes: Vec<u8>) -> String {
        let mut result = String::new();
        for pre in prefixes {
            let s = self.prefixes.get(&pre);
            if s.is_some() {
                result.push_str(&s.unwrap());
            }
        }
        result
    }

    pub fn format_reg(
        &self,
        index: usize,
        size: &OperandSize,
        group: &RegisterType,
        rex: bool,
    ) -> String {
        let mut result;
        match group {
            RegisterType::GPReg => {
                result = if *size != OperandSize::Byte || rex {
                    self.rex_gp_set[index].clone()
                } else {
                    // These should only be used for byte operations
                    self.gp_set[index].clone()
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
                result = self.mm_base.clone();
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
                result = self.k_base.clone();
                result += &index.to_string();
            }

            RegisterType::BoundReg => {
                result = self.bnd_base.clone();
                if index < 4 {
                    result += &index.to_string();
                } else {
                    panic!("Invalid register index for bounds register");
                }
            }

            RegisterType::SegReg => {
                // REX bits are ignored for seg regs
                if index > 0b101 {
                    // Assume 2 bit encoding for reserved segment accesses
                    result = self.seg_set[index & 0b0011].clone();
                } else {
                    result = self.seg_set[index & 0b0111].clone();
                }
            }

            RegisterType::FPUReg => {
                result = self.fpu_base.clone();
                result += &index.to_string();
            }

            RegisterType::CtrlReg => {
                result = self.ctrl_base.clone();
                // CR8 is only accessable when REX.R is set
                if rex {
                    result += "8"
                } else {
                    result += &index.to_string();
                }
            }

            RegisterType::DbgReg => {
                result = self.dbg_base.clone();
                result += &index.to_string();
            }
        }
        if !self.reg_uppercase {
            result = result.to_lowercase();
        }
        result
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
            println!("{}", inc.custom_format(&self.format));
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
                prefixes: None,
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
                prefixes: None,
            };
        }
        // Format instruction
        let operands = self.parse_operands(&instruction);
        let pref_size = if instruction.prefixes.is_some() {
            instruction.prefixes.as_ref().unwrap().len()
        } else {
            0
        };
        let start_offset = -((instruction.size + pref_size + operands.size) as isize);
        ParseResponse {
            instruction: instruction.val,
            operands: operands.val,
            bytes: Some(Vec::from(self.code.get_slice_offset(start_offset, 0))),
            prefixes: instruction.prefixes,
        }
    }

    fn parse_modrm(
        &mut self,
        reg: &RegisterType,
        size: &OperandSize,
        modrm: &Modrm,
        rex: &Rex,
        ins_size: &mut usize,
    ) -> Operand {
        if modrm.mode == 0b11 {
            // Explicit register
            return Operand::Reg(Register {
                index: (modrm.rm | rex.b) as usize,
                size: size.clone(),
                group: reg.clone(),
                rex: self.context.rex.is_some(),
            });
        } else if modrm.mode == 0 && modrm.rm == 0b101 {
            // Special case: immidiate offset
            let disp = if self.context.rex.is_none() && self.context.op_override {
                *ins_size += 2;
                self.parse_imm(2)
            } else {
                *ins_size += 4;
                self.parse_imm(4)
            };
            return Operand::Addr(Address {
                dest_size: size.clone(),
                addr_size: self.context.addr_size(),
                base: 0,
                index: None,
                scale: SIBScale::Zero,
                rm_disp: Some(disp),
                sib_disp: None,
                no_base: true,
            });
        } else {
            let mut addr = Address {
                dest_size: size.clone(),
                addr_size: self.context.addr_size(),
                base: 0,
                index: None,
                scale: SIBScale::Zero,
                rm_disp: None,
                sib_disp: None,
                no_base: false,
            };
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
                    addr.index = Some((index | rex.x) as usize);
                    addr.scale = match scale {
                        1 => SIBScale::Double,
                        2 => SIBScale::Quad,
                        3 => SIBScale::Octo,
                        _ => SIBScale::Zero,
                    }
                }
                // Base
                if base != 0b101 {
                    addr.base = (base | rex.b) as usize;
                } else {
                    // When base is 0b101 it means either it's based on RBP or a
                    // displacement, depending on mod
                    // Base reg based on arch size and prefixes
                    addr.base = 5;
                    match modrm.mode {
                        // Just displacement
                        0 => {
                            addr.no_base;
                            addr.sib_disp = Some(self.parse_imm(4));
                            *ins_size += 4;
                        }
                        // disp8 + ebp
                        1 => {
                            addr.sib_disp = Some(self.parse_imm(1));
                            *ins_size += 1;
                        }
                        // disp32 + ebp
                        // all this to enable C local variabes. Very cool
                        2 => {
                            addr.sib_disp = Some(self.parse_imm(4));
                            *ins_size += 4;
                        }
                        _ => {}
                    }
                }
            } else {
                // Normal base reg
                addr.base = (modrm.rm | rex.b) as usize;
            }
            match modrm.mode {
                0b1 => {
                    *ins_size += 1;
                    addr.rm_disp = Some(self.parse_imm(1));
                }
                0b10 => {
                    *ins_size += 4;
                    addr.rm_disp = Some(self.parse_imm(4));
                }
                _ => {}
            }
            return Operand::Addr(addr);
        }
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
        // # of bytes comprising the opperands
        let mut size = 0;
        let mut operands: Vec<Operand> = Vec::new();
        // We have to store this ahead of time because it encodes two values, modrm and modreg, and
        // the order of these operands isn't consistant, so modreg may be parsed before or after
        // modrm, which may advance the decoding to parse an SIB byte, ergo we can't rely on the
        // current code to be the modrm byte
        let modrm = Modrm {
            mode: (self.code.get() & 0b11000000) >> 6,
            reg: (self.code.get() & 0b00111000) >> 3,
            rm: (self.code.get() & 0b00000111),
        };
        let mut has_modrm = false;
        for op in instruction.operands.as_ref().unwrap() {
            // Consider prefixes for ops of unspecified size
            let real_size = if op.size != OperandSize::Any {
                &op.size
            } else if rex.w {
                &OperandSize::Quad
            } else if self.context.rex.is_none() && self.context.op_override {
                &OperandSize::Word
            } else {
                &OperandSize::Double
            };
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
                    operands.push(self.parse_modrm(real_reg, real_size, &modrm, &rex, &mut size));
                }
                OperandEncoding::Modreg => {
                    // To prevent modRM double count
                    if !has_modrm {
                        size += 1;
                        has_modrm = true;
                        // Advance to potential SIB byte
                        self.code.inc();
                    }
                    operands.push(Operand::Reg(Register {
                        index: (modrm.reg | rex.r) as usize,
                        size: *real_size,
                        group: op.reg.as_ref().unwrap_or(&RegisterType::GPReg).clone(),
                        rex: self.context.rex.is_some(),
                    }));
                }
                OperandEncoding::Opcode => {
                    operands.push(Operand::Reg(Register {
                        // Get last byte of opcode, logical and to get last 3 bits, include REX
                        // prefix, cast to usize for type jit
                        index: ((self.code.get_offset(-((size + 1) as isize)) & 0b00000111) | rex.b)
                            as usize,
                        size: *real_size,
                        group: op.reg.as_ref().unwrap_or(&RegisterType::GPReg).clone(),
                        rex: self.context.rex.is_some(),
                    }));
                }
                OperandEncoding::Immediate => {
                    operands.push(Operand::Imm(match real_size {
                        OperandSize::Byte => {
                            size += 1;
                            self.parse_imm(1)
                        }
                        OperandSize::Word => {
                            size += 2;
                            self.parse_imm(2)
                        }
                        OperandSize::Double => {
                            size += 4;
                            self.parse_imm(4)
                        }
                        OperandSize::DoubleSeg => {
                            size += 6;
                            self.parse_imm(6)
                        }
                        OperandSize::Quad => {
                            size += 8;
                            self.parse_imm(8)
                        }
                        OperandSize::Penta => {
                            size += 10;
                            self.parse_imm(10)
                        }
                        OperandSize::DoubleQuad => {
                            size += 16;
                            self.parse_imm(16)
                        }
                        OperandSize::QuadQuad => {
                            size += 32;
                            self.parse_imm(32)
                        }
                        OperandSize::Z => {
                            size += 48;
                            self.parse_imm(48)
                        }
                        OperandSize::DoubleQuadQuad => {
                            size += 64;
                            self.parse_imm(64)
                        }
                        OperandSize::Any => {
                            panic!("Immediate value size cannot be infered");
                        }
                    }));
                }
                OperandEncoding::Bespoke => {
                    let is_reg = Regex::new("([ER]?[AC]X)|([AC][HL])").unwrap();
                    let reg_mem_size_dif = Regex::new("r(8|16|32|64)/m(8|16|32|64)").unwrap();
                    // If register literal
                    if is_reg.is_match(&op.text) {
                        let mut op_str = op.text.clone();
                        if !self.format.reg_uppercase {
                            op_str = op_str.to_lowercase();
                        }
                        operands.push(Operand::Bes(Bespoke { value: op_str }));
                    } else if op.text == "mib" {
                        // Some sort of evil subset of SIB addressing
                        let base = self.code.get() & 0b111;
                        self.code.inc();
                        size += 1;
                        let mut addr = Address {
                            dest_size: real_size.clone(),
                            addr_size: self.context.addr_size(),
                            base: 0,
                            index: None,
                            scale: SIBScale::Zero,
                            rm_disp: None,
                            sib_disp: None,
                            no_base: false,
                        };
                        if base == 0b101 {
                            // Displacment
                            addr.base = 5; // Code for base pointer
                            match modrm.mode {
                                // Just displacement
                                0 => {
                                    addr.no_base = true;
                                    addr.sib_disp = Some(self.parse_imm(4));
                                    size += 4;
                                }
                                // disp8 + ebp
                                1 => {
                                    addr.sib_disp = Some(self.parse_imm(1));
                                    size += 1;
                                }
                                // disp32 + ebp
                                2 => {
                                    addr.sib_disp = Some(self.parse_imm(4));
                                    size += 4;
                                }
                                _ => {}
                            }
                        } else {
                            addr.base = (base | rex.b) as usize;
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
                            operands.push(self.parse_modrm(
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
                            ));
                        } else {
                            // Memory, sized according to op
                            operands.push(
                                self.parse_modrm(real_reg, &op.size, &modrm, &rex, &mut size),
                            );
                        }
                    } else if op.text.contains("16:") {
                        // Far pointer
                        let real_reg = if op.reg == Some(RegisterType::MMXReg) {
                            &RegisterType::MMXReg
                        } else {
                            &RegisterType::GPReg
                        };
                        if op.text.starts_with("ptr") {
                            // Full address is stored as immediate
                            match self.context.addr_size() {
                                OperandSize::Word => {
                                    operands.push(Operand::Imm(self.parse_imm(6)));
                                    size += 6
                                }
                                OperandSize::Double => {
                                    operands.push(Operand::Imm(self.parse_imm(6)));
                                    size += 6
                                }
                                // This encoding isn't valid in 64 bit mode
                                _ => {}
                            }
                        } else {
                            // Full address is read at given memory addr
                            if !has_modrm {
                                size += 1;
                                has_modrm = true;
                                // Advance to potential SIB byte
                                self.code.inc();
                            }
                            match self.context.addr_size() {
                                OperandSize::Word => {
                                    operands.push(self.parse_modrm(
                                        real_reg,
                                        &OperandSize::Double,
                                        &modrm,
                                        &rex,
                                        &mut size,
                                    ));
                                }
                                OperandSize::Double => {
                                    operands.push(self.parse_modrm(
                                        real_reg,
                                        &OperandSize::DoubleSeg,
                                        &modrm,
                                        &rex,
                                        &mut size,
                                    ));
                                }
                                OperandSize::Quad => {
                                    operands.push(self.parse_modrm(
                                        real_reg,
                                        &OperandSize::Penta,
                                        &modrm,
                                        &rex,
                                        &mut size,
                                    ));
                                }
                                _ => {}
                            }
                        }
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
                        operands
                            .push(self.parse_modrm(real_reg, real_size, &modrm, &rex, &mut size));
                    } else {
                        println!("{:#?}", instruction);
                        panic!("Unknown bespoke");
                    }
                }
            }
        }
        if operands.is_empty() {
            OperandResponse {
                ..Default::default()
            }
        } else {
            OperandResponse {
                val: Some(operands),
                size,
            }
        }
    }

    fn parse_imm(&mut self, count: usize) -> Immediate {
        let mut i = 0;
        let mut val: u64 = 0;
        while i < count {
            val += (self.code.get() as u64) << (i * 8);
            self.code.inc();
            i += 1;
        }
        return Immediate { value: val };
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
                prefixes: None,
            };
        }
        // Figure out the prefixes
        for byte in &prefix {
            // If byte isn't in range to be a valid prefix then escape
            if (byte & 0b11110000) == 0b01000000 && self.context.size == ArchSize::I64 {
                self.context.rex = Some(Rex::from(*byte));
            } else if *byte < 0x26 || *byte > 0xf3 {
                break;
            } else if *byte >= 0xf0 {
                self.context.one = *byte;
            } else if *byte == 0x66 {
                self.context.op_override = true;
            } else if *byte == 0x67 {
                self.context.addr_override = true;
            } else {
                self.context.two = match *byte {
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
                    } else if valids[i].size == OperandSize::Quad
                        && (self.context.op_override || self.context.addr_override)
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
                prefixes: None,
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
                prefixes: Some(prefix),
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
                prefixes: Some(prefix),
            };
        } else if valids[0].text.starts_with("PUSH") {
            // PUSHF vs PUSHF(QD) is evil
            // If op size is non defualt use 16 bit
            if self.context.op_override {
                for i in 0..valids.len() {
                    if valids[i].text.ends_with("F") {
                        let mut rep = valids[i].clone();
                        if !self.format.ins_uppercase {
                            rep.text = rep.text.to_lowercase();
                        }
                        return InstructionResponse {
                            val: Some(rep),
                            size,
                            prefixes: Some(prefix),
                        };
                    }
                }
            } else {
                for i in 0..valids.len() {
                    if !valids[i].text.ends_with("F") {
                        let mut rep = valids[i].clone();
                        if !self.format.ins_uppercase {
                            rep.text = rep.text.to_lowercase();
                        }
                        return InstructionResponse {
                            val: Some(rep),
                            size,
                            prefixes: Some(prefix),
                        };
                    }
                }
            }
        } else if valids[0].text.starts_with("INS")
            || valids[0].text.starts_with("OUTS")
            || valids[0].text.starts_with("SCAS")
            || valids[0].text.starts_with("LODS")
            || (valids[0].text.starts_with("C") && valids[0].description.contains("sign-extend"))
        {
            if self.context.op_override {
                for i in 0..valids.len() {
                    if valids[i].text.ends_with("W") {
                        let mut rep = valids[i].clone();
                        if !self.format.ins_uppercase {
                            rep.text = rep.text.to_lowercase();
                        }
                        return InstructionResponse {
                            val: Some(rep),
                            size,
                            prefixes: Some(prefix),
                        };
                    }
                }
            } else {
                for i in 0..valids.len() {
                    if valids[i].text.ends_with("D") || valids[i].text.ends_with("DE") {
                        let mut rep = valids[i].clone();
                        if !self.format.ins_uppercase {
                            rep.text = rep.text.to_lowercase();
                        }
                        return InstructionResponse {
                            val: Some(rep),
                            size,
                            prefixes: Some(prefix),
                        };
                    }
                }
            }
        } else if valids[0].text.starts_with("IN ") {
            if self.context.op_override {
                for i in 0..valids.len() {
                    if valids[i].text.contains(" AX") {
                        let mut rep = valids[i].clone();
                        if !self.format.ins_uppercase {
                            rep.text = rep.text.to_lowercase();
                        }
                        return InstructionResponse {
                            val: Some(rep),
                            size,
                            prefixes: Some(prefix),
                        };
                    }
                }
            } else {
                for i in 0..valids.len() {
                    if valids[i].text.contains("EAX") {
                        let mut rep = valids[i].clone();
                        if !self.format.ins_uppercase {
                            rep.text = rep.text.to_lowercase();
                        }
                        return InstructionResponse {
                            val: Some(rep),
                            size,
                            prefixes: Some(prefix),
                        };
                    }
                }
            }
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
                        prefixes: Some(prefix),
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

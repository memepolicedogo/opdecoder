#![allow(dead_code, unused)]
mod instruction_tree;
use serde_json;
use std::{env, fs, io};

use crate::instruction_tree::{ArchSize, ByteString, Context, Decoder};

const REXW: u8 = 0b01001000;

enum OutputFormat {
    PrettyPrint,
    JSON,
}

struct Options {
    //----INPUT OPTIONS----
    // Path to tree JSON
    tree_path: String,
    // What arch size to instantiate the context with
    arch_size: ArchSize,
    // Where to read input from ('-' for stdin)
    input: String,
    // How many bytes of input to ignore, meant for file where there are headers and data above the
    // the code
    input_offset: u64,
    // How many bytes to read total, normally will just go till EOF
    read_max: u64,
    //---------------------
    //---OUTPUT OPTIONS----
    // Where to output ('-' for stdout)
    output: String,
    // What format, e.g. json if you want to parse the instructions with another program
    output_format: OutputFormat,
    //---------------------
}

impl Default for Options {
    fn default() -> Self {
        Self {
            tree_path: String::new(),
            arch_size: ArchSize::I64,
            input: String::from("-"),
            input_offset: 0,
            read_max: 0,
            output: String::from("-"),
            output_format: OutputFormat::PrettyPrint,
        }
    }
}

fn main() {
    test();
    return;
    // Parse CLI args
    let mut opts = Options {
        ..Default::default()
    };
    // Get all but the first arg
    let args: Vec<String> = env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        // Iter by index so we can access the next val as needed
        match args[i].as_str() {
            "-t" | "--tree" => {
                opts.tree_path = args[i + 1].clone();
                i += 1;
            }
            "-a" | "--arch" => {
                opts.arch_size = parse_arch(args[i + 1].as_str());
                i += 1;
            }
            _ => {
                println!("Unknown arg \"{}\"", args[i]);
                return;
            }
        }
        i += 1;
    }
}

fn parse_arch(arch: &str) -> ArchSize {
    match arch {
        "64" | "I64" | "i64" | "X64" | "x64" => ArchSize::I64,
        "32" | "I32" | "i32" | "X32" | "x32" => ArchSize::I32,
        "16" | "I16" | "i16" | "X16" | "x16" => ArchSize::I16,
        _ => {
            panic!("Invalid arch size");
        }
    }
}

fn test() {
    let mut dec = Decoder {
        context: Context {
            ..Default::default()
        },
        tree: serde_json::from_str(&fs::read_to_string("tree2.json").expect("AHH")).expect("AHHH"),
        code: ByteString {
            code: vec![0x48, 0x83, 0xf8, 0x01],
            curr: 0,
        },
    };
    let mut rep = dec.parse_one();
    println!("Match:");
    rep.pretty_print();
    rep.print_bytes();
}

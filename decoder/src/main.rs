#![allow(dead_code, unused)]
mod instruction_tree;
use serde_json;
use std::{env, fs, io};

use crate::instruction_tree::{ArchSize, ByteString, Context, Decoder, InstructionTree};

const REXW: u8 = 0b01001000;

#[derive(Debug)]
enum OutputFormat {
    PrettyPrint,
    JSON,
}

#[derive(Debug)]
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
    //let mut tree = InstructionTree::from_json(&fs::read_to_string("reduced.json").unwrap());
    //fs::write("tree3.json", serde_json::to_string(&tree).unwrap());
    test();
    return;
    // Parse CLI args
    let mut opts = Options {
        ..Default::default()
    };
    // Get all but the first arg
    let mut args: Vec<String> = env::args().skip(1).collect();
    let mut i = 0;
    // Iter by index so we can access the next val as needed
    while i < args.len() {
        // Split {opt}={val} into seperate args
        if args[i].contains("=") {
            let tmp = args[i].clone();
            let split: Vec<&str> = tmp.split('=').collect();
            args[i] = String::from(split[0]);
            args.insert(i + 1, String::from(split[1]));
        }
        match args[i].as_str() {
            "-t" | "--tree" => {
                i += 1;
                opts.tree_path = args[i].clone();
            }
            "-a" | "--arch" => {
                i += 1;
                opts.arch_size = parse_arch(args[i].as_str());
            }
            "-i" | "--input" => {
                i += 1;
                opts.input = args[i].clone();
            }
            "--offset" => {
                i += 1;
                let offset = u64::from_str_radix(args[i].as_str(), 10);
                if offset.is_err() {
                    println!("Invalid offset");
                    return;
                }
                opts.input_offset = offset.unwrap();
            }
            "-m" | "--max" => {
                i += 1;
                let max = u64::from_str_radix(args[i].as_str(), 10);
                if max.is_err() {
                    println!("Invalid max");
                    return;
                }
                opts.read_max = max.unwrap();
            }
            "-o" | "--output" => {
                i += 1;
                opts.output = args[i].clone();
            }
            "-f" | "--format" => {
                i += 1;
                opts.output_format = parse_format(args[i].as_str());
            }
            _ => {
                println!("Unknown arg \"{}\"", args[i]);
                return;
            }
        }
        i += 1;
    }

    let bytes = if opts.input != "-" {
        fs::read(opts.input).unwrap()
    } else {
        Vec::new()
    };

    let tree_str = &fs::read_to_string(opts.tree_path);
    if tree_str.is_err() {
        println!("Invalid tree path");
        return;
    }

    let mut dec = Decoder {
        context: Context {
            size: opts.arch_size,
            ..Default::default()
        },
        tree: serde_json::from_str(&tree_str.unwrap()).expect("Invalid tree JSON"),
        code: ByteString {
            code: bytes,
            curr: 0,
        },
    };
}

fn parse_arch(arch: &str) -> ArchSize {
    match arch.to_lowercase().as_str() {
        "64" | "i64" | "x64" => ArchSize::I64,
        "32" | "i32" | "x32" => ArchSize::I32,
        "16" | "i16" | "x16" => ArchSize::I16,
        _ => {
            panic!("Invalid arch size");
        }
    }
}

fn parse_format(format: &str) -> OutputFormat {
    match format.to_lowercase().as_str() {
        "print" | "p" => OutputFormat::PrettyPrint,
        "json" | "j" => OutputFormat::JSON,
        _ => {
            panic!("Invalid output format");
        }
    }
}

fn test() {
    let mut test_one = vec![
        0x58, 0x48, 0x83, 0xf8, 0x01, 0x0F, 0x84, 0x04, 0x04, 0x00, 0x00, 0x00,
    ];
    let mut test_two = vec![
        0x48, 0xf7, 0xe3, 0x4c, 0x01, 0xd8, 0x4d, 0x31, 0xd2, 0x00, 0x00,
    ];
    let mut test_three = vec![
        0x58, 0x48, 0x83, 0xF8, 0x01, 0x0F, 0x84, 0x04, 0x04, 0x00, 0x00, 0x48, 0x83, 0xF8, 0x02,
        0x0F, 0x84, 0xC1, 0x04, 0x00, 0x00, 0x48, 0x83, 0xF8, 0x03, 0x0F, 0x8C, 0x30, 0x04, 0x00,
        0x00, 0x48, 0x89, 0x04, 0x25, 0xCC, 0x22, 0x40, 0x00, 0x58, 0x48, 0x89, 0x04, 0x25, 0xD4,
        0x22, 0x40, 0x00, 0x48, 0xFF, 0xC0, 0x80, 0x38, 0x00, 0x75, 0xF8, 0x48, 0x83, 0x3C, 0x25,
        0xCC, 0x22, 0x40, 0x00, 0x03, 0x0F, 0x84, 0x34, 0x01, 0x00, 0x00, 0x48, 0x83, 0x3C, 0x25,
        0xCC, 0x22, 0x40, 0x00, 0x04, 0x74, 0x06, 0x0F, 0x85, 0xD7, 0x03, 0x00, 0x00, 0x48, 0xFF,
        0xC0, 0x80, 0x38, 0x30, 0x0F, 0x8C, 0x55, 0x04, 0x00, 0x00, 0x80, 0x38, 0x39, 0x0F, 0x8F,
        0x4C, 0x04, 0x00, 0x00, 0x44, 0x8A, 0x10, 0x41, 0x80, 0xEA, 0x30, 0x48, 0xFF, 0xC0, 0x4C,
        0x89, 0x14, 0x25, 0xDE, 0x22, 0x40, 0x00, 0x80, 0x38, 0x00, 0x74, 0x5C, 0x80, 0x38, 0x30,
        0x0F, 0x8C, 0x2C, 0x04, 0x00, 0x00, 0x80, 0x38, 0x39, 0x0F, 0x8F, 0x23, 0x04, 0x00, 0x00,
        0x44, 0x8A, 0x18, 0x41, 0x80, 0xEB, 0x30, 0x48, 0x89, 0x04, 0x25, 0xD4, 0x22, 0x40, 0x00,
        0x4C, 0x89, 0xD0, 0xBB, 0x0A, 0x00, 0x00, 0x00, 0x48, 0xF7, 0xE3, 0x4C, 0x01, 0xD8, 0x4D,
        0x31, 0xD2, 0x4D, 0x31, 0xDB, 0x48, 0x83, 0xF8, 0x01, 0x0F, 0x8C, 0xF6, 0x03, 0x00, 0x00,
        0x48, 0x83, 0xF8, 0x24, 0x0F, 0x8F, 0xEC, 0x03, 0x00, 0x00, 0x48, 0x89, 0x04, 0x25, 0xDE,
        0x22, 0x40, 0x00, 0x48, 0x8B, 0x04, 0x25, 0xD4, 0x22, 0x40, 0x00, 0x00,
    ];
    let mut dec = Decoder {
        context: Context {
            ..Default::default()
        },
        tree: serde_json::from_str(&fs::read_to_string("tree2.json").expect("AHH")).expect("AHHH"),
        code: ByteString {
            code: test_one,
            curr: 0,
        },
    };
    dec.parse_n_print();
    return;
    let mut reps = dec.parse();
    for rep in reps {
        rep.pretty_print();
        rep.print_bytes();
    }
}

#![allow(dead_code, unused)]
mod instruction_tree;
use serde_json;
use std::{fs, io};

const REXW: u8 = 0b01001000;

fn main() {
    let mut dec = instruction_tree::Decoder {
        context: instruction_tree::Context {
            ..Default::default()
        },
        tree: serde_json::from_str(&fs::read_to_string("tree2.json").expect("AHH")).expect("AHHH"),
    };
    let mut code = vec![0x58, 0x48, 0x83, 0xf8, 0x01];
    let mut rep = dec.parse_instruction(&code);
    let mut ins = rep.val.expect("Poop from a butt");
    println!("Match:");
    println!("Offset: {}", rep.offset);
    println!("{}:", ins.text);
    println!("\t{}", ins.opcode);
    println!("\t{}", ins.description);
    let mut i = rep.offset;
    while i != 0 {
        println!("Poped value from code");
        code.remove(0);
        i -= 1;
    }
    rep = dec.parse_instruction(&code);
    ins = rep.val.expect("Poop from a butt");
    println!("Match:");
    println!("Offset: {}", rep.offset);
    println!("{}:", ins.text);
    println!("\t{}", ins.opcode);
    println!("\t{}", ins.description);
}

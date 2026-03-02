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
        tree: serde_json::from_str(&fs::read_to_string("tree.json").expect("AHH")).expect("AHHH"),
    };
    let code = vec![0x66, 0x0f, 0x1a];
    let rep = dec.parse(&code).val.expect("Poop from a butt");
    println!("Match:");
    println!("{}:", rep.text);
    println!("\t{}", rep.opcode);
    println!("\t{}", rep.description);
}

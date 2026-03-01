#![allow(dead_code, unused)]
mod instruction_tree;
use std::{fs, io};

const REXW: u8 = 0b01001000;

fn main() {
    //instruction_tree::InstructionTree::from_json(&fs::read_to_string("/home/samuel/Documents/Coding/opdecoder/reduced.json").expect("AHHH"),);

    let mut dec = instruction_tree::Decoder {
        context: instruction_tree::Context {
            ..Default::default()
        },
        tree: serde_json::from_str(&fs::read_to_string("tree.json").expect("AHH")).expect("AHHH"),
    };
    let code = vec![0xf3, 0x0f, 0x38, 0xd8, 0x20];
    let rep = dec.parse(&code).val.expect("Poop from a butt");
    println!("Match:");
    println!("{}:", rep.text);
    println!("\t{}", rep.opcode);
    println!("\t{}", rep.description);
}

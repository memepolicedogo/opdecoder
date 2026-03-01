#![allow(dead_code, unused)]
mod instruction_tree;
use std::{fs, io};

const REXW: u8 = 0b01001000;

fn main() {
    let mut tree: instruction_tree::InstructionTree =
        serde_json::from_str(&fs::read_to_string("tree.json").expect("AHH")).expect("AHHH");
    //instruction_tree::InstructionTree::from_json(&fs::read_to_string("/home/samuel/Documents/Coding/opdecoder/reduced.json").expect("AHHH"),);

    let code = vec![0xf3, 0x0f, 0x38, 0xd8];
    let rep = tree.traverse(&code);
    print!("Match");
    if rep.val.len() > 1 {
        print!("es");
    }
    println!(":");
    for instruction in rep.val {
        println!("{}:", instruction.text);
        println!("\t{}", instruction.opcode);
        println!("\t{}", instruction.description);
    }
}

#![allow(dead_code, unused)]
mod instruction_tree;
use std::{fs, io};

const REXW: u8 = 0b01001000;

fn main() {
    let mut tree = instruction_tree::InstructionTree::from_json(
        &fs::read_to_string("/home/samuel/Documents/Coding/opdecoder/reduced.json").expect("AHHH"),
    );
    let mut x = true;
    while x {
        println!("Enter a op byte in hex");
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("Failed to read line");
        let rep = tree.step(
            u8::from_str_radix(input.strip_suffix('\n').expect("Fucekd up"), 16)
                .expect("Invalid Hex"),
        );
        x = !rep.bottom;
        print!("Found {} Matching Instruction", rep.val.len());
        if rep.val.len() > 1 {
            print!("s");
        }
        println!("");
        if !x {
            println!("Reached bottom of tree");
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
    }
}

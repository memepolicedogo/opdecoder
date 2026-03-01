mod instruction_tree;
use std::fs;

fn main() {
    let mut tree = instruction_tree::InstructionTree::from_json(
        &fs::read_to_string("/home/samuel/Documents/Coding/opdecoder/mov.json").expect("AHHH"),
    );
    println!("{:?}", tree.step(0x8c));
}

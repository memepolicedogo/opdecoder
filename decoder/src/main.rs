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
        code: instruction_tree::ByteString {
            code: vec![0x48, 0x83, 0xf8, 0x01],
            curr: 0,
        },
    };
    let mut rep = dec.parse_one();
    println!("Match:");
    rep.pretty_print();
    rep.print_bytes();
}

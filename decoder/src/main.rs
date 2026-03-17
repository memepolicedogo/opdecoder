#![allow(dead_code, unused)]
mod instruction_tree;
use core::panic;
use goblin::{Object, elf::Elf, pe::PE};
use serde_json;
use std::{
    env,
    fs::{self, File},
    io::{self, IsTerminal, Read, Seek, SeekFrom, Write},
};

use crate::instruction_tree::{ArchSize, ByteString, Context, Decoder, InstructionTree};

#[derive(Debug, PartialEq, PartialOrd)]
enum OutputFormat {
    PlusBytes,
    PrettyPrint,
    JSON,
}

#[derive(Debug)]
struct Options {
    //----INPUT OPTIONS----
    // Path to tree JSON
    tree_path: String,
    tree_path_static: bool,
    // What arch size to instantiate the context with
    arch_size: ArchSize,
    arch_size_static: bool,
    // Where to read input from ('-' for stdin)
    input: String,
    input_static: bool,
    // How many bytes of input to ignore, meant for file where there are headers and data above the
    // the code
    input_offset: u64,
    input_offset_static: bool,
    // How many bytes to read total, normally will just go till EOF
    read_max: u64,
    read_max_static: bool,
    // Should the program use file headers to find the executable section rather than cli args
    no_infer: bool,
    //---------------------
    //---OUTPUT OPTIONS----
    // Where to output ('-' for stdout)
    output: String,
    output_static: bool,
    // What format, e.g. json if you want to parse the instructions with another program
    output_format: OutputFormat,
    output_format_static: bool,
    // How many instructions to display
    ins_max: usize,
    ins_max_static: bool,
    //---------------------
}

impl Default for Options {
    fn default() -> Self {
        Self {
            tree_path: String::from("./tree64.json"),
            tree_path_static: false,
            arch_size: ArchSize::I64,
            arch_size_static: false,
            input: String::from("-"),
            input_static: false,
            input_offset: 0,
            input_offset_static: false,
            read_max: 0,
            read_max_static: false,
            no_infer: false,
            output: String::from("-"),
            output_static: false,
            output_format: OutputFormat::PrettyPrint,
            output_format_static: false,
            ins_max: 0,
            ins_max_static: false,
        }
    }
}

//{
const HELP_MSG: &str = "
    usage: decoder [OPTIONS]

    Options:
        -t, --tree      Specify the JSON instruction tree to load
                        Default: ./tree64.json
        -a, --arch      Specify the architecture size (16, 32, 64)
                        Default: x64
        -i, --input     Specify the input file or \"-\" for stdin 
                        Default: -
        --offset        Specify the offset in byte from the start of the file to start dissassembly
                        Default: 0
        -m, --max       Specify the maximum number of bytes to read from the file, or 0 to read to EOF
                        Default: 0
        -o, --ouput     Specify the output file or \"-\" for stdout
                        Default: -
        -f, --format    Specify the output format (PrettyPrint, PlusBytes, JSON)
                        Default: PrettyPrint
        -l, --lines     Specify the number of lines to parse, or 0 for all
                        Default: 0
        --no-infer      Do not attempt parse executable headers from the input when max and offset == 0
                        Default: false
        -h, --help      Display this help message and exit

    Common commands:
        decoder -i {exe_path} -o {file}.json -f j
            Decode the file at {exe_path}, infering executable sections from the headers, and write a JSON representation
            of the result to {file}.json

    Notes:
        CLI args take precendent over infered values, e.g. \"decoder -i some.exe -m 100\" will infer the start of the 
            executable section and read the first 100 bytes regardless of the size of the section
        Stdin cannot be used interactivly
        Format specifiers are not case sensitive
        Max and Offset are ignored in stdin mode
";
//}

fn main() {
    // Parse CLI args
    let mut opts = Options {
        ..Default::default()
    };
    // Get all but the first arg
    let mut args: Vec<String> = env::args().skip(1).collect();
    if args.len() == 0 {
        println!("{}", HELP_MSG);
        return;
    }
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
                opts.tree_path_static = true;
            }
            "-a" | "--arch" => {
                i += 1;
                opts.arch_size = parse_arch(args[i].as_str());
                opts.arch_size_static = true;
            }
            "-i" | "--input" => {
                i += 1;
                opts.input = args[i].clone();
                opts.input_static = true;
            }
            "--offset" => {
                i += 1;
                let offset = u64::from_str_radix(args[i].as_str(), 10);
                if offset.is_err() {
                    println!("Invalid offset");
                    return;
                }
                opts.input_offset = offset.unwrap();
                opts.input_offset_static = true;
            }
            "-m" | "--max" => {
                i += 1;
                let max = u64::from_str_radix(args[i].as_str(), 10);
                if max.is_err() {
                    println!("Invalid max");
                    return;
                }
                opts.read_max = max.unwrap();
                opts.read_max_static = true;
            }
            "--no-infer" => {
                opts.no_infer = true;
            }
            "-o" | "--output" => {
                i += 1;
                opts.output = args[i].clone();
                opts.output_static = true;
            }
            "-f" | "--format" => {
                i += 1;
                opts.output_format = parse_format(args[i].as_str());
                opts.output_format_static = true;
            }
            "-l" | "--lines" => {
                i += 1;
                let max = usize::from_str_radix(args[i].as_str(), 10);
                if max.is_err() {
                    println!("Invalid line max");
                    return;
                }
                opts.ins_max = max.unwrap();
                opts.ins_max_static = true;
            }
            "-h" | "--help" => {
                println!("{}", HELP_MSG);
                return;
            }
            _ => {
                println!("Unknown arg \"{}\", use -h for help", args[i]);
                return;
            }
        }
        i += 1;
    }

    let tree_str = &fs::read_to_string(&opts.tree_path);
    if tree_str.is_err() {
        println!("Invalid tree path");
        return;
    }

    let mut dec = Decoder {
        context: Context {
            size: match opts.arch_size {
                ArchSize::I16 => ArchSize::I16,
                ArchSize::I32 => ArchSize::I32,
                ArchSize::I64 => ArchSize::I64,
            },
            ..Default::default()
        },
        tree: serde_json::from_str(&tree_str.as_ref().unwrap()).expect("Invalid tree JSON"),
        code: ByteString {
            code: Vec::new(),
            curr: 0,
        },
    };

    // Load data
    if opts.input == "-" {
        load_from_stdin(&mut dec, &opts);
    } else {
        load_from_file(&mut dec, &mut opts);
    }
    if !dec.has_code() && opts.input == "-" {
    } else if !dec.has_code() {
        panic!("No code was loaded, check your input options");
    }
    // Get write object for output
    let mut output = open_output(&opts.output);
    let responses = if opts.ins_max == 0 {
        dec.parse()
    } else {
        dec.parse_n(opts.ins_max)
    };
    match opts.output_format {
        OutputFormat::JSON => {
            let json = serde_json::to_string(&responses);
            if json.is_err() {
                println!("Failed to serialize response data:");
                println!("{}", &json.unwrap_err());
                return;
            }
            let _ = write!(output, "{}", json.unwrap());
        }
        OutputFormat::PrettyPrint => {
            for rep in responses {
                let _ = writeln!(output, "{}", rep);
            }
        }
        OutputFormat::PlusBytes => {
            for rep in responses {
                let _ = writeln!(output, "{}", rep.bytes_to_string());
                let _ = writeln!(output, "{}", rep);
            }
        }
    }
}

fn open_output(path: &String) -> Box<dyn Write> {
    if path == "-" {
        Box::new(io::stdout())
    } else {
        Box::new(File::create(path).expect("Bad output file"))
    }
}

fn load_from_stdin(dec: &mut Decoder, opts: &Options) {
    let stdin = io::stdin();
    if io::Stdin::is_terminal(&stdin) {
        panic!("Can't be run interactivly, Specify a file or pipe data in");
    } else {
        // Load in stdin as code
        dec.load_code(&stdin.bytes().map(|x| x.unwrap()).collect());
    }
}

fn load_from_file(dec: &mut Decoder, opts: &mut Options) {
    // Get file
    let mut file = fs::File::open(&opts.input).expect("Invalid input file");
    // Infer size as needed
    if !opts.no_infer {
        let buf = fs::read(&opts.input).unwrap();
        match Object::parse(&buf).unwrap_or(Object::Unknown(0)) {
            Object::Elf(elf) => {
                opts_from_elf(&elf, opts);
            }
            Object::PE(pe) => {
                opts_from_pe(&pe, opts);
            }
            _ => {}
        }
    }
    // Seek to offset
    let _ = file.seek(SeekFrom::Start(opts.input_offset));
    // Load code
    let mut code: Vec<u8> = Vec::new();
    let _ = file.read_to_end(&mut code);
    if opts.read_max != 0 {
        code.drain((opts.read_max as usize)..);
    }
    dec.load_code(&code);
}

const PE_SUPPORTED_MACHINES: [u16; 2] = [
    0x8664, // x86_64
    0x14c,  // x86_32
];
fn opts_from_pe(pe: &PE, opts: &mut Options) {
    if !PE_SUPPORTED_MACHINES.contains(&pe.header.coff_header.machine) {
        panic!(
            "Unsupported machine type \"{}\"",
            pe.header.coff_header.machine
        );
    }
    if !opts.arch_size_static {
        if pe.is_64 {
            opts.arch_size = ArchSize::I64;
        } else {
            opts.arch_size = ArchSize::I32;
        }
    }
    for sec in &pe.sections {
        if sec.name[1] == 116 {
            // 't'
            if !opts.input_offset_static {
                opts.input_offset = sec.pointer_to_raw_data as u64
            }
            if !opts.read_max_static {
                opts.read_max = sec.size_of_raw_data as u64 + 1;
            }
        }
    }
}

const ELF_SUPPORTED_MACHINES: [u16; 2] = [
    3,  // EM_386 - x86_32
    62, // EM_X86_64 - x86_64
];
fn opts_from_elf(elf: &Elf, opts: &mut Options) {
    if !ELF_SUPPORTED_MACHINES.contains(&elf.header.e_machine) {
        panic!("Unsupported machine type");
    }
    if !opts.arch_size_static {
        if elf.is_64 {
            opts.arch_size = ArchSize::I64;
        } else {
            opts.arch_size = ArchSize::I32;
        }
    }
    for header in &elf.program_headers {
        // R_X
        if header.p_flags == 5 {
            if !opts.input_offset_static {
                opts.input_offset = header.p_offset;
            }
            if !opts.read_max_static {
                opts.read_max = header.p_filesz + 1;
            }
            return;
        }
    }
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
        "bytes" | "byte" | "b" => OutputFormat::PlusBytes,
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
    let mut test_four = vec![0x80, 0x38, 0x00, 0x00];
    let mut dec = Decoder {
        context: Context {
            ..Default::default()
        },
        tree: serde_json::from_str(&fs::read_to_string("tree3.json").expect("AHH")).expect("AHHH"),
        code: ByteString {
            code: test_four,
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

fn build_tree() {
    let mut tree =
        InstructionTree::from_json(&fs::read_to_string("instructions/x86_reduced.json").unwrap());
    fs::write("tree32.json", serde_json::to_string(&tree).unwrap());
    return;
}

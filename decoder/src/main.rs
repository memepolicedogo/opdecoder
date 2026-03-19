#![allow(dead_code, unused)]
mod instruction_tree;
use core::panic;
use goblin::{Object, elf::Elf, pe::PE};
use serde_json;
use std::{
    collections::HashMap,
    env, fmt,
    fs::{self, File},
    hash::Hash,
    io::{self, IsTerminal, Read, Seek, SeekFrom, Write},
};

use crate::instruction_tree::{
    ArchSize, ByteString, Context, Decoder, InstructionFormatting, InstructionTree,
};

#[derive(Debug, PartialEq, PartialOrd)]
enum OutputFormat {
    PlusBytes,
    PrettyPrint,
    JSON,
}

enum ArgValue {
    Text(String),
    Bool(bool),
}

impl fmt::Display for ArgValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArgValue::Bool(x) => write!(f, "{}", x),
            ArgValue::Text(x) => write!(f, "{}", x),
        }
    }
}

struct Argument {
    name: String,
    description: String,
    default: ArgValue,
    flags: Vec<String>,
    value: Option<ArgValue>,
}

struct Arguments {
    help_msg: String,
    raw_args: Vec<Argument>,
    flags: HashMap<String, usize>,
    names: HashMap<String, usize>,
}

impl Arguments {
    fn help(&self) {
        println!("{}", self.help_msg);
    }
    fn from(args: Vec<Argument>, header: &str, footer: &str) -> Self {
        let mut flags: HashMap<String, usize> = HashMap::new();
        let mut names: HashMap<String, usize> = HashMap::new();
        let mut help_msg = String::from(header);
        for i in 0..args.len() {
            // Build flag based hashmap
            for flag in &args[i].flags {
                flags.insert(flag.clone(), i);
            }
            // Build name based hashmap
            names.insert(args[i].name.clone(), i);
            // Build help message
            help_msg += &format!("{}", &args[i]);
        }
        help_msg += footer;
        Self {
            help_msg,
            raw_args: args,
            flags,
            names,
        }
    }

    fn match_flag(&mut self, flag: &String) -> Option<&mut Argument> {
        let i = self.flags.get(flag);
        if i.is_none() {
            None
        } else {
            Some(&mut self.raw_args[*i.unwrap()])
        }
    }

    fn get(&mut self, name: &str) -> &mut Argument {
        let i = self.names.get(name);
        &mut self.raw_args[*i.unwrap()]
    }

    fn get_val(&self, name: &str) -> &ArgValue {
        let i = self.names.get(name);
        &self.raw_args[*i.unwrap()].get()
    }
}

impl fmt::Display for Argument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut fmt_flags = String::from("");
        for flag in &self.flags {
            fmt_flags += flag;
            fmt_flags += ", ";
        }
        fmt_flags = String::from(fmt_flags.strip_suffix(", ").unwrap_or(""));

        write!(
            f,
            "\t{flags:<20} {description}
\t                     Default: {default}\n",
            flags = fmt_flags,
            description = self.description,
            default = self.default,
        )
    }
}

impl Argument {
    fn get(&self) -> &ArgValue {
        self.value.as_ref().unwrap_or(&self.default)
    }

    fn get_str(&self) -> &String {
        match &self.default {
            ArgValue::Bool(x) => panic!("Can't get a str from a bool argument"),
            ArgValue::Text(x) => &x,
        }
    }

    fn get_usize(&self) -> usize {
        match &self.default {
            ArgValue::Bool(x) => panic!("Can't get a str from a bool argument"),
            ArgValue::Text(x) => usize::from_str_radix(x, 10).expect("Invalid argument"),
        }
    }

    fn get_bool(&self) -> bool {
        match &self.default {
            ArgValue::Text(x) => panic!("Can't get a bool from a str argument"),
            ArgValue::Bool(x) => x.clone(),
        }
    }

    fn new(name: &str, description: &str, default: ArgValue, flags: Vec<&str>) -> Self {
        Self {
            name: String::from(name),
            description: String::from(description),
            default,
            flags: flags.iter().map(|x| String::from(*x)).collect(),
            value: None,
        }
    }
}

//{
const HELP_HEADER: &str = "usage: decoder [OPTIONS]

Options:
";

const HELP_FOOTER: &str = "
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
    // Build options
    let mut opts = Arguments::from(
        vec![
            Argument::new(
                "tree",
                "The path to a JSON instruction tree",
                ArgValue::Text(String::from("./tree64.json")),
                vec!["-t", "--tree"],
            ),
            Argument::new(
                "arch",
                "The architecture size (16, 32, or 64 bit)",
                ArgValue::Text(String::from("x64")),
                vec!["-a", "--arch"],
            ),
            Argument::new(
                "input",
                "The input file or \"-\" for stdin",
                ArgValue::Text(String::from("-")),
                vec!["-i", "--input"],
            ),
            Argument::new(
                "offset",
                "The number of bytes to ignore before parsing",
                ArgValue::Text(String::from("0")),
                vec!["--offset"],
            ),
            Argument::new(
                "max",
                "The maximum number of bytes to read while parsing, or 0 for no limit",
                ArgValue::Text(String::from("0")),
                vec!["-m", "--max"],
            ),
            Argument::new(
                "output",
                "The path to an output file, or \"-\" for stdout",
                ArgValue::Text(String::from("-")),
                vec!["-o", "--output"],
            ),
            Argument::new(
                "format",
                "How the decoded data should be presented (PrettyPrint, PlusBytes, JSON)",
                ArgValue::Text(String::from("PrettyPrint")),
                vec!["-f", "--format"],
            ),
            Argument::new(
                "custom",
                "JSON (either a file or an inline JSON string) describing the formating of the instructions",
                ArgValue::Text(String::from("")),
                vec!["-c", "--custom"],
            ),
            Argument::new(
                "lines",
                "The maximum number of instructions to parse, or 0 for unlimited",
                ArgValue::Text(String::from("0")),
                vec!["-l", "--lines"],
            ),
            Argument::new(
                "no-infer",
                "Do not parse executable headers to find code regions, use arguments/defaults",
                ArgValue::Bool(false),
                vec!["--no-infer"],
            ),
            Argument::new(
                "help",
                "Print this help message and exit",
                ArgValue::Text(String::from("")),
                vec!["-h", "--help"],
            ),
        ],
        HELP_HEADER,
        HELP_FOOTER,
    );
    // Parse CLI args
    // Get all but the first arg
    let mut args: Vec<String> = env::args().skip(1).collect();
    if args.len() == 0 {
        opts.help();
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

        match opts.match_flag(&args[i]) {
            Some(x) => {
                if x.name == "help" {
                    opts.help();
                    return;
                }
                match x.default {
                    ArgValue::Text(_) => {
                        i += 1;
                        x.value = Some(ArgValue::Text(args[i].clone()));
                    }
                    ArgValue::Bool(_) => {
                        x.value = Some(ArgValue::Bool(true));
                    }
                }
            }
            None => {
                panic!("Invalid option \"{}\"", args[i]);
            }
        }
        i += 1;
    }

    let tree_str = &fs::read_to_string(&opts.get("tree").get_str());
    if tree_str.is_err() {
        println!("Invalid tree path");
        return;
    }

    let formatting = if opts.get("custom").get_str() == "" {
        InstructionFormatting {
            ..Default::default()
        }
    } else if fs::exists(opts.get("custom").get_str()).unwrap_or(false) {
        serde_json::from_str(&fs::read_to_string(opts.get("custom").get_str()).unwrap())
            .expect("Invalid formatting file")
    } else {
        serde_json::from_str(opts.get("custom").get_str()).expect("Invalid formatting string")
    };

    let mut dec = Decoder {
        context: Context {
            size: parse_arch(opts.get("arch").get_str()),
            ..Default::default()
        },
        format: formatting,
        tree: serde_json::from_str(&tree_str.as_ref().unwrap()).expect("Invalid tree JSON"),
        code: ByteString {
            code: Vec::new(),
            curr: 0,
        },
    };

    // Load data
    if opts.get("input").get_str() == "-" {
        load_from_stdin(&mut dec, &opts);
    } else {
        load_from_file(&mut dec, &mut opts);
    }

    if opts.get("input").get_str() == "-" {
    } else if !dec.has_code() {
        panic!("No code was loaded, check your input options");
    }
    // Get write object for output
    let mut output = open_output(opts.get("output").get_str());
    let instruction_max = opts.get("lines").get_usize();
    let responses = if instruction_max == 0 {
        dec.parse()
    } else {
        dec.parse_n(instruction_max)
    };

    let output_format = parse_format(opts.get("format").get_str());

    match output_format {
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

fn load_from_stdin(dec: &mut Decoder, opts: &Arguments) {
    let stdin = io::stdin();
    if io::Stdin::is_terminal(&stdin) {
        panic!("Can't be run interactivly, Specify a file or pipe data in");
    } else {
        // Load in stdin as code
        dec.load_code(&stdin.bytes().map(|x| x.unwrap()).collect());
    }
}

fn load_from_file(dec: &mut Decoder, opts: &mut Arguments) {
    // Get file
    let mut file = fs::File::open(opts.get("input").get_str()).expect("Invalid input file");
    // Infer size as needed
    if !opts.get("no-infer").get_bool() {
        let buf = fs::read(opts.get("input").get_str()).unwrap();
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
    let _ = file.seek(SeekFrom::Start(opts.get("offset").get_usize() as u64));
    // Load code
    let mut code: Vec<u8> = Vec::new();
    let _ = file.read_to_end(&mut code);
    if opts.get("max").get_usize() != 0 {
        code.drain((opts.get("max").get_usize())..);
    }
    dec.load_code(&code);
}

const PE_SUPPORTED_MACHINES: [u16; 2] = [
    0x8664, // x86_64
    0x14c,  // x86_32
];
fn opts_from_pe(pe: &PE, opts: &mut Arguments) {
    if !PE_SUPPORTED_MACHINES.contains(&pe.header.coff_header.machine) {
        panic!(
            "Unsupported machine type \"{}\"",
            pe.header.coff_header.machine
        );
    }
    if opts.get("arch").value.is_none() {
        if pe.is_64 {
            opts.get("arch").value = Some(ArgValue::Text(String::from("64")));
        } else {
            opts.get("arch").value = Some(ArgValue::Text(String::from("32")));
        }
    }
    for sec in &pe.sections {
        if sec.name[1] == 116 {
            // 't'
            if opts.get("offset").value.is_none() {
                opts.get("offset").value =
                    Some(ArgValue::Text(sec.pointer_to_raw_data.to_string()));
            }
            if opts.get("max").value.is_none() {
                opts.get("max").value =
                    Some(ArgValue::Text((sec.pointer_to_raw_data + 1).to_string()));
            }
        }
    }
}

const ELF_SUPPORTED_MACHINES: [u16; 2] = [
    3,  // EM_386 - x86_32
    62, // EM_X86_64 - x86_64
];
fn opts_from_elf(elf: &Elf, opts: &mut Arguments) {
    if !ELF_SUPPORTED_MACHINES.contains(&elf.header.e_machine) {
        panic!("Unsupported machine type");
    }

    if opts.get("arch").value.is_none() {
        if elf.is_64 {
            opts.get("arch").value = Some(ArgValue::Text(String::from("64")));
        } else {
            opts.get("arch").value = Some(ArgValue::Text(String::from("32")));
        }
    }

    for header in &elf.program_headers {
        // R_X
        if header.p_flags == 5 {
            if opts.get("offset").value.is_none() {
                opts.get("offset").value = Some(ArgValue::Text(header.p_offset.to_string()));
            }
            if opts.get("max").value.is_none() {
                opts.get("max").value = Some(ArgValue::Text((header.p_filesz + 1).to_string()));
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
        "print" | "p" | "prettyprint" => OutputFormat::PrettyPrint,
        "json" | "j" => OutputFormat::JSON,
        "bytes" | "byte" | "b" | "plusbytes" => OutputFormat::PlusBytes,
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
        format: InstructionFormatting {
            ..Default::default()
        },
        tree: serde_json::from_str(&fs::read_to_string("tree64.json").expect("AHH")).expect("AHHH"),
        code: ByteString {
            code: test_three,
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

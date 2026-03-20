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
use textwrap::fill;

use crate::instruction_tree::{
    ArchSize, ByteString, Context, Decoder, InstructionFormatting, InstructionTree,
};

#[derive(Debug, PartialEq, PartialOrd)]
enum OutputFormat {
    PlusBytes,
    PrettyPrint,
    JSON,
}

#[derive(Debug)]
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

#[derive(Debug)]
struct Argument {
    name: String,
    description: String,
    help: String,
    default: ArgValue,
    flags: Vec<String>,
    value: Option<ArgValue>,
}

impl Argument {
    fn get(&self) -> &ArgValue {
        self.value.as_ref().unwrap_or(&self.default)
    }

    fn get_str(&self) -> &String {
        match &self.value.as_ref().unwrap_or(&self.default) {
            ArgValue::Bool(x) => panic!("Can't get a str from a bool argument"),
            ArgValue::Text(x) => &x,
        }
    }

    fn get_usize(&self) -> usize {
        match &self.value.as_ref().unwrap_or(&self.default) {
            ArgValue::Bool(x) => panic!("Can't get a uszie from a bool argument"),
            ArgValue::Text(x) => usize::from_str_radix(x, 10).expect("Invalid argument"),
        }
    }

    fn get_bool(&self) -> bool {
        match &self.value.as_ref().unwrap_or(&self.default) {
            ArgValue::Text(x) => panic!("Can't get a bool from a str argument"),
            ArgValue::Bool(x) => x.clone(),
        }
    }

    fn new(name: &str, description: &str, help: &str, default: ArgValue, flags: Vec<&str>) -> Self {
        Self {
            name: String::from(name),
            description: String::from(description),
            help: String::from(help),
            default,
            flags: flags.iter().map(|x| String::from(*x)).collect(),
            value: None,
        }
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
        let mut desc_options = textwrap::Options::new(80);
        desc_options.subsequent_indent = "\t                     ";
        write!(
            f,
            "\t{flags:<20} {description}
\t                     Default: {default}\n",
            flags = fmt_flags,
            description = fill(&self.description, desc_options),
            default = self.default,
        )
    }
}

#[derive(Debug)]
struct Arguments {
    help_msg: String,
    raw_args: Vec<Argument>,
    flags: HashMap<String, usize>,
    names: HashMap<String, usize>,
}

impl Arguments {
    fn help(&self) {
        println!("{}", fill(&self.help_msg, 100));
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

//{
const HELP_HEADER: &str = "usage: decoder [OPTIONS]

Options:
";

const HELP_FOOTER: &str = "
Common commands:
    decoder -i {exe_path} -o {file}.json -f j
        Decode the file at {exe_path}, infering executable sections from the headers, and write a 
        JSON representation of the result to {file}.json
";
//}

fn main() {
    // Build options
    let mut opts = Arguments::from(
        vec![
            Argument::new(
                "tree",
                "The path to a JSON instruction tree",
                "The path to a file containing a JSON representation of the instruction tree. This tree is used for mapping an opcode to an instruction, after which the instruction data from the tree can be used to determine the operands of the instruction. It is the heart of this disassembler. The goal of saving this tree as a JSON file is to enhance flexability, though unfortuantly because of the complexity of the x86 ISA, it is unlikley if not impossible that this program could be used to disassemble a different ISA using only a different tree. The main value of this flexability is in the ability to use a smaller version of the tree for improved performance when it is known that the code being disassembled doesn't use some subset of the instruction set.",
                ArgValue::Text(String::from("./tree64.json")),
                vec!["-t", "--tree"],
            ),
            Argument::new(
                "arch",
                "The architecture size (16, 32, or 64 bit)",
                "What version of the x86 architecture the code is written for. Can be either 16, 32, or 64 bit. Currently only 32 and 64 bit are supported.",
                ArgValue::Text(String::from("x64")),
                vec!["-a", "--arch"],
            ),
            Argument::new(
                "input",
                "The input file or \"-\" for stdin",
                "Where the data to be decoded comes from, either a file path or \"-\" for stdin. You cannot use stdin interactivly, the data must be piped in.",
                ArgValue::Text(String::from("-")),
                vec!["-i", "--input"],
            ),
            Argument::new(
                "offset",
                "The number of bytes to ignore before parsing",
                "The number of bytes that will be ignored and not loaded to be parsed into instructions. For file inputs this is achived by seeking within the file before the read, whereas for stdin it reads the skipped bytes into a buffer that is discarded.",
                ArgValue::Text(String::from("0")),
                vec!["--offset"],
            ),
            Argument::new(
                "max",
                "The maximum number of bytes to read while parsing, or 0 for no limit",
                "The max bytes that will be read from the input and loaded for decoding. Note that this is not affected by the offset, e.g. --offset 100 --max 50 will load the 50 bytes following the offset. This differs from lines in that it is purely bytewise, making no attempt to align to the end of an instruction, and that it affects the number of bytes actually read from the file, and will result in lower memory usage for the same output if used carefully.",
                ArgValue::Text(String::from("0")),
                vec!["-m", "--max"],
            ),
            Argument::new(
                "output",
                "The path to an output file, or \"-\" for stdout",
                "Where the data will be written, must be either a valid file path or \"-\" for stdout",
                ArgValue::Text(String::from("-")),
                vec!["-o", "--output"],
            ),
            Argument::new(
                "format",
                "How the decoded data should be presented (PrettyPrint, PlusBytes, JSON)",
                "What format should the output be, one of PrettyPrint, PlusBytes, or JSON. This differs from custom in that it doesn't change the data produced by the decoder, only the actual output of the program. What each type represents is specified below.
PrettyPrint: Outputs only the assembly instructions line by line
PlusBytes: Same as PrettyPrint, but with the bytes associated with the instruction on the line above the instruction
JSON: A JSON string containing all of the data generated by the decoder, with the following structure:
[
// If the decoder fails to find a valid instruction then instruction and operands in the following
// structure will be null and the bytes array will have a single byte
{
    instrution: { // Generic data on the instruction, built from the Intel 64 and IA-32 Architectures Software Development Manual, or null 
        opcode: \"\",
        text: \"\",
        x64: true,
        legacy: false,
        operands: [ 
    {
            size: 64, // In bits
            encoding: \"\" // One of Opcode, Immediate, Modrm, Modreg, Bespoke
            reg: \"\" // One of null, GPReg, SegReg, FPUReg, MMXReg, BoundReg, KReg
            text: \"\"
    },
    ],
        size: 64, // In bits
        invalid_prefixes: [0x66],
        description: \"\",
    },
    operands: [\"\"], // The specific operands for this instance of the instruction, or null
    bytes: [0x00], // The bytes making up this instance of the instruction
},
]
",
                ArgValue::Text(String::from("PrettyPrint")),
                vec!["-f", "--format"],
            ),
            Argument::new(
                "custom",
                "JSON (either a file or an inline JSON string) describing the formating of the instructions",
                "This JSON data is used to determine the formatting of the text representation of the instructions. This differs from format in that it changes the data produced by the decoder, and only indirectly affects the actual output. If a field is not specified in the JSON the default is used. The fields and their defaults are specified below.
{
    reg_uppercase: true, // Should registers be uppercase
    imm_uppercase: true, // Should letters in immediate values be uppercase
    addr_open: \"[\", // String appended directly before a memory address
    addr_close: \"]\", // String appended directly after a memory address
    addr_add: \"+\", // String used to denote addition in effective address calculation
    addr_mul: \"*\", // String used to denote multiplication in effective address calculation
    addr_scale_two: \"2\", // String used to denote two in effective address calculation with SIB byte
    addr_scale_four: \"4\", // String used to denote four in effective address calculation with SIB byte
    addr_scale_eight: \"8\", // String used to denote eight in effective address calculation with SIB byte
    addr_prefix: \"\", // String prepended to a memory address 
    addr_byte: \"byte \", // String prepended to 8 bit memory accesses
    addr_word: \"word \", // String prepended to 16 bit memory accesses
    addr_dword: \"dword \", // String prepended to 32 bit memory accesses
    addr_qword: \"qword \", // String prepended to 64 bit memory accesses
    addr_tword: \"tword \", // String prepended to 80 bit memory accesses
    addr_oword: \"oword \", // String prepended to 128 bit memory accesses
    addr_yword: \"yword \", // String prepended to 256 bit memory accesses
    addr_zword: \"zword \", // String prepended to 512 bit memory accesses
    imm_prefix: \"0x\", // String prepended to an immediate value
    imm_suffix: \"\", // String appended to an immediate value
    imm_fmt: \"Hex\", // Format for immedate values, one of Hex, Dec, Bi, Oct (base 16, 10, 2, and 8 respectivly)
    ins_uppercase: true, // Should the text of the instruction itself be uppercase
    code_fmt: \"Hex\", // Format for opcode bytes, one of Hex, Dec, Bi, Oct (base 16, 10, 2, and 8 respectivly) 
}
The default values produce NASM-style assembly
",
                ArgValue::Text(String::from("")),
                vec!["-c", "--custom"],
            ),
            Argument::new(
                "lines",
                "The maximum number of instructions to parse, or 0 for unlimited",
                "This option provides a maximum number of instructions to parse. Setting it to any value other than 0 will cause the parser to decode instructions until either it reaches the end of the data or it has reached the limit. This differs from max in that it doesn't effect how many bytes are read from the file, it only acts as a logical check.",
                ArgValue::Text(String::from("0")),
                vec!["-l", "--lines"],
            ),
            Argument::new(
                "no-infer",
                "Do not parse executable headers to find code regions, use arguments/defaults",
                "Enabling this option will skip the code that parses executable headers to find where the executable region of the code starts and how large it is. This code uses the same variables as the offset and max arguments, and checks if a value was passed as an argument, so if you specify values for max and offset this option is redundent.",
                ArgValue::Bool(false),
                vec!["--no-infer"],
            ),
            Argument::new(
                "help",
                "Display a detailed help message about this program or about specific arguments",
                "Display a detailed help message about this program or about specific arguments",
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
                    i += 1;
                    if i == args.len() {
                        opts.help();
                        return;
                    } else {
                        let target = if opts.match_flag(&args[i]).is_some() {
                            opts.match_flag(&args[i]).unwrap()
                        } else {
                            opts.get(&args[i])
                        };
                        println!(
                            "{}\n{}\n",
                            textwrap::dedent(&format!("{}", target)),
                            fill(&target.help, 100)
                        );
                        return;
                    }
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
        load_from_stdin(&mut dec, &mut opts);
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

fn load_from_stdin(dec: &mut Decoder, opts: &mut Arguments) {
    let stdin = io::stdin();
    if io::Stdin::is_terminal(&stdin) {
        panic!("Can't be run interactivly, Specify a file or pipe data in");
    } else {
        // Load in stdin as code
        let mut stdin_bytes: Vec<u8> = stdin.bytes().map(|x| x.unwrap()).collect();
        stdin_bytes.drain(0..opts.get("offset").get_usize());
        if opts.get("max").get_usize() != 0 {
            stdin_bytes.drain(opts.get("max").get_usize()..);
        }
        stdin_bytes.push(0);
        dec.load_code(&stdin_bytes);
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
            "Unsupported machine type: \"{}\"",
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
        panic!("Unsupported machine type: \"{}\"", elf.header.e_machine);
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
    let mut test_bytes = vec![0xff, 0b00101000, 0];
    let mut dec = Decoder {
        context: Context {
            ..Default::default()
        },
        format: InstructionFormatting {
            ..Default::default()
        },
        tree: serde_json::from_str(&fs::read_to_string("tree64.json").expect("AHH")).expect("AHHH"),
        code: ByteString {
            code: test_bytes,
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
    let mut tree64 =
        InstructionTree::from_json(&fs::read_to_string("instructions/x64_reduced.json").unwrap());
    fs::write("tree64.json", serde_json::to_string(&tree64).unwrap());
    return;
}

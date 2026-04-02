#![allow(dead_code, unused)]
mod instruction_tree;
use bevy_reflect::{
    DynamicStruct, GetPath, PartialReflect, Reflect, TypeRegistry, serde::ReflectSerializer,
};
use core::panic;
use goblin::{
    Object,
    elf::{Elf, SectionHeader},
    pe::{PE, section_table::SectionTable},
};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;
use serde_json::{self, to_string};
use std::{
    collections::HashMap,
    env,
    fmt::{self, Display},
    fs::{self, File},
    hash::Hash,
    io::{self, IsTerminal, Read, Seek, SeekFrom, Write},
    str::FromStr,
};
use textwrap::fill;

use crate::instruction_tree::{
    ArchSize, ByteString, Context, CustomFormat, Decoder, InstructionFormatting, InstructionTree,
    ParseResponse,
};

#[derive(Debug, PartialEq, PartialOrd)]
enum OutputFormat {
    PlusBytes,
    PrettyPrint,
    JSON,
}

#[derive(Debug, PartialEq)]
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
    fn is_default(&self) -> bool {
        self.value.is_none()
    }

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

#[derive(Serialize, Reflect, Clone)]
struct Section {
    name: String,
    offset: usize,
    size: usize,
    addr: u64,
    disassembled: Vec<ParseResponse>,
}

impl Section {
    fn from_elf(elf: &SectionHeader, name: String) -> Self {
        Self {
            name,
            offset: elf.sh_offset as usize,
            size: elf.sh_size as usize,
            addr: elf.sh_addr,
            disassembled: Vec::new(),
        }
    }

    fn from_pe(pe: &SectionTable, base: u64) -> Self {
        Self {
            name: pe.real_name.as_ref().unwrap().clone(),
            offset: pe.size_of_raw_data as usize,
            size: pe.pointer_to_raw_data as usize,
            addr: (pe.virtual_address as u64 + base),
            disassembled: Vec::new(),
        }
    }

    fn abs_addr(&self, rvaddr: u64) -> u64 {
        self.addr + rvaddr
    }
}

#[derive(Serialize, Reflect, Clone)]
struct Symbol {
    name: String,
    value: u64,
}

impl PartialEq for Symbol {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.value == other.value
    }
}

impl PartialEq<&String> for Symbol {
    fn eq(&self, other: &&String) -> bool {
        if let Ok(o) = u64::from_str_radix(other, 16) {
            self.value == o
        } else {
            false
        }
    }
}

impl PartialEq<String> for Symbol {
    fn eq(&self, other: &String) -> bool {
        if let Ok(o) = u64::from_str_radix(other, 16) {
            self.value == o
        } else {
            false
        }
    }
}

#[derive(Serialize, Reflect, Clone)]
struct Executable {
    path: String,
    code: Vec<Section>,
    syms: Vec<Symbol>,
    entry: usize,
    is_64: bool,
}

impl From<String> for Executable {
    fn from(path: String) -> Self {
        Executable::from(&path)
    }
}

impl From<&String> for Executable {
    fn from(path: &String) -> Self {
        let mut exe = Self {
            path: path.clone(),
            code: Vec::new(),
            syms: Vec::new(),
            entry: 0,
            is_64: true,
        };
        let mut file = fs::File::open(path).unwrap();
        let mut buff = Vec::new();
        file.read_to_end(&mut buff);
        // Parse file headers to pull all executable sections
        match Object::parse(&buff).unwrap_or(Object::Unknown(0)) {
            Object::Elf(elf) => {
                exe.is_64 = elf.is_64;
                for sym in elf.syms.iter() {
                    if sym.st_name != 0 {
                        exe.syms.push(Symbol {
                            name: String::from(elf.strtab.get_at(sym.st_name).unwrap_or("")),
                            value: sym.st_value,
                        });
                    }
                }
                for sec in elf.section_headers {
                    if sec.is_executable() {
                        // If the entry point is contained within this section
                        if elf.entry >= sec.sh_addr && elf.entry <= (sec.sh_addr + sec.sh_size) {
                            exe.entry = exe.code.len();
                        }
                        exe.code.push(Section::from_elf(
                            &sec,
                            String::from(elf.shdr_strtab.get_at(sec.sh_name).unwrap()),
                        ));
                    }
                }
            }
            Object::PE(pe) => {
                exe.is_64 = pe.is_64;
                for sec in pe.sections {
                    if (sec.characteristics & 0x20) == 0x20 {
                        // If the entry point is contained within this section
                        if pe.entry >= sec.virtual_address as usize
                            && pe.entry <= (sec.virtual_address + sec.virtual_size) as usize
                        {
                            exe.entry = exe.code.len();
                        }
                        exe.code.push(Section::from_pe(&sec, pe.image_base as u64))
                    }
                }
            }
            _ => exe.code.push(Section {
                name: String::from("arbitrary"),
                offset: 0,
                size: 0,
                addr: 0,
                disassembled: Vec::new(),
            }),
        }
        exe
    }
}

impl Executable {
    fn replace_symbol(&self, val: String) -> String {
        for sym in &self.syms {
            let symstr = format!("{:X}", sym.value);
            if val.contains(&symstr) {
                return val.replace(&symstr, &sym.name);
            }
        }
        return val;
    }

    // Resolve differences between executable and passed CLI options
    fn resolve_opts(&mut self, opts: &mut Arguments) {
        // Use CLI options for raw files and when no-infer is set
        if opts.get("no-infer").get_bool()
            || (self.code.len() == 1 && self.code[0].name == "arbitrary")
        {
            self.code = vec![Section {
                name: String::from("arbitrary"),
                offset: opts.get("offset").get_usize(),
                size: opts.get("max").get_usize(),
                addr: 0,
                disassembled: Vec::new(),
            }];
            self.is_64 = opts.get("size").get_usize() == 64;
        }
    }
}

#[derive(Reflect)]
struct InterContext {
    source: Executable,
    opts: InterOptions,
    format: InstructionFormatting,
}

#[derive(Reflect)]
struct InterOptions {
    tree: String,
    output: String,
    format: String,
    arch: String,
}

impl InterContext {
    fn build(exe: &Executable, opts: &mut Arguments, format: &InstructionFormatting) -> Self {
        Self {
            source: exe.clone(),
            opts: InterOptions {
                tree: opts.get("tree").get_str().clone(),
                output: opts.get("output").get_str().clone(),
                format: opts.get("format").get_str().clone(),
                arch: opts.get("arch").get_str().clone(),
            },
            format: format.clone(),
        }
    }
}

fn main() {
    let mut opts = match handle_args() {
        Some(x) => x,
        None => return,
    };

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

    let path = if fs::exists(opts.get("input").get_str()).unwrap_or(false) {
        opts.get("input").get_str()
    } else {
        &file_from_path(&opts.get("input").get_str())
    };
    // Open file
    let mut exe = Executable::from(path);
    exe.resolve_opts(&mut opts);
    let mut file = fs::File::open(&exe.path).unwrap();

    if opts.get("interactive").get_bool() {
        println!("Interactive mode is still in development. Sorry!");
        // Set up environment
        let mut root = InterContext::build(&exe, &mut opts, &formatting);
        println!("Found {} executable section(s) in file", exe.code.len());

        //----Interactive planning----
        //
        // Commands:
        // Step through parsing a la GDB
        // Parse arbitrary hex strings
        // Write to files (?)
        let mut active = true;
        while active {
            // Print cursor
            let mut input = String::new();
            print!("> ");
            io::stdout().flush();
            io::stdin().read_line(&mut input).expect("Failed to read");
            let (cmd, mut args) = match parse_command(&input) {
                Some(tuple) => tuple,
                None => {
                    println!("Unknown command");
                    continue;
                }
            };
            match cmd {
                InterCmd::Exit => {
                    println!("Exiting");
                    return;
                }
                InterCmd::Print => intr_print(&mut root, args),
                InterCmd::Set => intr_set(&mut root, args),
                InterCmd::Parse => {
                    // parse {name/index}
                    println!("Preparing decoder...");
                    let mut output = open_output(&root.opts.output);
                    let tree_str = &fs::read_to_string(&opts.get("tree").get_str());
                    if tree_str.is_err() {
                        println!("Invalid tree path");
                        continue;
                    }
                    let tree = match serde_json::from_str(&tree_str.as_ref().unwrap()) {
                        Ok(x) => x,
                        Err(e) => {
                            println!("Invalid tree JSON");
                            continue;
                        }
                    };
                    let mut dec = Decoder {
                        context: Context {
                            size: parse_arch(&root.opts.arch),
                            ..Default::default()
                        },
                        format: root.format.clone(),
                        tree,
                        code: ByteString {
                            code: Vec::new(),
                            curr: 0,
                        },
                    };
                    let mut sects: Vec<usize> = Vec::new();
                    println!("Decoder loaded");
                    let mut parsing = true;
                    while parsing {
                        print!("Decoder> ");
                        io::stdout().flush();
                        input = String::new();
                        io::stdin().read_line(&mut input).expect("Failed to read");
                        let (cmd, mut args) = match parse_command(&input) {
                            Some(tuple) => tuple,
                            None => {
                                println!("Unknown command");
                                continue;
                            }
                        };
                        match cmd {
                            InterCmd::Exit => {
                                println!("Exiting decoder");
                                break;
                            }
                            InterCmd::Print => intr_print(&mut root, args),
                            InterCmd::Set => intr_set(&mut root, args),
                            InterCmd::Load => {
                                // Load section
                                if args.is_none() || args.as_ref().unwrap() == "" {
                                    // Parse all
                                    for i in 0..root.source.code.len() {
                                        sects.push(i);
                                        println!("Loaded section {}", root.source.code[i].name);
                                    }
                                } else if let Ok(i) = usize::from_str(args.as_ref().unwrap()) {
                                    // Parse index
                                    if i >= root.source.code.len() {
                                        println!("Invalid index");
                                        continue;
                                    }
                                    sects.push(i);
                                    println!("Loaded section {}", root.source.code[i].name);
                                } else {
                                    // Parse name
                                    for i in 0..root.source.code.len() {
                                        if root.source.code[i].name == *args.as_ref().unwrap() {
                                            sects.push(i);
                                            println!("Loaded section {}", args.unwrap());
                                            break;
                                        }
                                    }
                                }
                            }
                            InterCmd::Step => {
                                // Step through parsing process
                                for index in &sects {
                                    file.seek(SeekFrom::Start(
                                        root.source.code[*index].offset as u64,
                                    ));
                                    let mut buff = Vec::new();
                                    let x = file.read_to_end(&mut buff);
                                    buff.drain(root.source.code[*index].size..);
                                    dec.load_code(&buff);
                                    let _ = writeln!(
                                        output,
                                        "{}",
                                        dec.format.as_section(&root.source.code[*index].name)
                                    );
                                    let mut parsed = dec.parse_one();
                                    let mut vaddr: u64 = 0;
                                    while parsed.bytes.is_some() {
                                        // Addr
                                        let _ = write!(
                                            output,
                                            "0x{:X}\t",
                                            root.source.code[*index].abs_addr(vaddr)
                                        );
                                        // Code
                                        output_one(
                                            &parsed,
                                            &mut output,
                                            &parse_format(opts.get("format").get_str()),
                                            &formatting,
                                        );
                                        // Increment addr
                                        if parsed.bytes.is_some() {
                                            vaddr += parsed.bytes.as_ref().unwrap().len() as u64;
                                        }
                                        root.source.code[*index].disassembled.push(parsed);
                                        io::stdin().read_line(&mut input).expect("Failed to read");
                                        parsed = dec.parse_one();
                                    }
                                }
                            }
                            InterCmd::Parse => {
                                // Start parsing
                                for index in &sects {
                                    file.seek(SeekFrom::Start(
                                        root.source.code[*index].offset as u64,
                                    ));
                                    let mut buff = Vec::new();
                                    let x = file.read_to_end(&mut buff);
                                    buff.drain(root.source.code[*index].size..);
                                    dec.load_code(&buff);
                                    root.source.code[*index].disassembled = dec.parse();
                                }
                                println!("Parsing");
                                output_parsed(
                                    &root.source,
                                    &mut output,
                                    &parse_format(opts.get("format").get_str()),
                                    &formatting,
                                );
                            }
                            _ => println!("Command not valid in this context"),
                        }
                    }
                }
                _ => println!("Command not valid in this context"),
            }
        }
    } else {
        let tree_str = &fs::read_to_string(&opts.get("tree").get_str());
        if tree_str.is_err() {
            println!("Invalid tree path");
            return;
        }
        let mut dec = Decoder {
            context: Context {
                size: parse_arch(opts.get("arch").get_str()),
                ..Default::default()
            },
            format: formatting.clone(),
            tree: serde_json::from_str(&tree_str.as_ref().unwrap()).expect("Invalid tree JSON"),
            code: ByteString {
                code: Vec::new(),
                curr: 0,
            },
        };
        // Get write object for output
        let mut output = open_output(opts.get("output").get_str());
        let instruction_max = opts.get("lines").get_usize();
        for mut section in &mut exe.code {
            // Get code
            writeln!(io::stderr(), "Loading section {}", section.name);
            file.seek(SeekFrom::Start(section.offset as u64));
            let mut buff = Vec::new();
            let x = file.read_to_end(&mut buff);
            buff.drain(section.size..);
            dec.load_code(&buff);
            writeln!(io::stderr(), "Loaded {} bytes", section.size);
            writeln!(io::stderr(), "Disassembling...");
            section.disassembled = if instruction_max == 0 {
                parse_with_progress(&mut dec)
            } else {
                dec.parse_n(instruction_max)
            };
            writeln!(io::stderr(), "{} instructions", section.disassembled.len());
        }

        output_parsed(
            &exe,
            &mut output,
            &parse_format(opts.get("format").get_str()),
            &formatting,
        );
    }
}

const INSTRUCTIONS_PER_UPDATE: usize = 5;
fn parse_with_progress(dec: &mut Decoder) -> Vec<ParseResponse> {
    // Create progress bar with max as the full size in bytes of the code
    let mut bar = ProgressBar::new(dec.code.len() as u64);
    bar.set_style(
        ProgressStyle::with_template(
            "{elapsed_precise} {bar:100.cyan} {bytes}/{total_bytes} \n{msg:110}{bytes_per_sec}",
        )
        .unwrap()
        .progress_chars("#&-"),
    );
    let mut dis = Vec::new();
    while !dec.code.is_end() {
        dis.append(&mut dec.parse_n(INSTRUCTIONS_PER_UPDATE));
        bar.set_position(dec.code.curr as u64);
    }
    bar.finish();
    dis
}

fn output_parsed(
    exe: &Executable,
    output: &mut Box<dyn Write>,
    format: &OutputFormat,
    opts: &InstructionFormatting,
) {
    match format {
        OutputFormat::JSON => {
            let json = serde_json::to_string(&exe);
            if json.is_err() {
                println!("Failed to serialize response data:");
                println!("{}", &json.unwrap_err());
                return;
            }
            let _ = write!(output, "{}", json.unwrap());
        }
        OutputFormat::PrettyPrint => {
            for mut sec in &exe.code {
                let _ = writeln!(output, "section {}", sec.name);
                let mut vaddr: u64 = 0;
                for rep in &sec.disassembled {
                    if rep.bytes.is_some() {
                        let tmp = rep.bytes.as_ref().unwrap();
                        if tmp.contains(&0xc6)
                            && tmp.contains(&0x0f)
                            && tmp.contains(&0x12)
                            && tmp.len() == 4
                        {
                            println!("{:#?}", rep);
                        }
                    }
                    // Print address
                    let _ = write!(output, "0x{:X}\t", sec.abs_addr(vaddr));
                    // Print code
                    output_one(rep, output, format, opts);
                    // Increment address
                    if rep.bytes.is_some() {
                        vaddr += rep.bytes.as_ref().unwrap().len() as u64;
                    }
                }
                write!(output, "\n");
            }
        }
        OutputFormat::PlusBytes => {
            for mut sec in &exe.code {
                let _ = writeln!(output, "{}", sec.name);
                for rep in &sec.disassembled {
                    output_one(rep, output, format, opts);
                }
                write!(output, "\n");
            }
        }
    }
}

fn output_one(
    response: &ParseResponse,
    output: &mut Box<dyn Write>,
    format: &OutputFormat,
    opts: &InstructionFormatting,
) {
    match format {
        OutputFormat::JSON => {
            println!("Invalid format for individual output");
        }
        OutputFormat::PrettyPrint => {
            let _ = writeln!(output, "{}", response.custom_format(opts));
            //output.flush();
        }
        OutputFormat::PlusBytes => {
            let _ = writeln!(output, "{}", response.bytes_to_string());
            let _ = writeln!(output, "{}", response.custom_format(opts));
            output.flush();
        }
    }
}

fn intr_set(root: &mut InterContext, args: Option<String>) {
    if args.is_none() {
        println!("Set requires two arugments");
        return;
    }
    let (dest, arg) = match args.as_ref().unwrap().split_once(" ") {
        Some(tuple) => tuple,
        None => {
            println!("Set requires two arugments");
            return;
        }
    };
    let res = root.reflect_path_mut(dest);
    match res {
        Ok(x) => {
            if set_reflect(x, &arg.to_string()) {
                println!("Set");
            } else {
                println!("Invalid value");
            }
        }
        Err(e) => println!("Invalid path"),
    }
}

fn intr_print(root: &mut InterContext, args: Option<String>) {
    if let Some(path) = args {
        let res = root.reflect_path(path.as_str());
        match res {
            Ok(x) => {
                println!("{:#?}", x);
            }
            Err(e) => println!("Invalid path"),
        }
    } else {
        println!("Invalid argument")
    }
}

fn set_reflect(val: &mut dyn PartialReflect, arg: &String) -> bool {
    if let Some(x) = val.try_downcast_mut::<usize>() {
        let y = usize::from_str(arg);
        if y.is_ok() {
            *x = y.unwrap();
            return true;
        }
        return false;
    } else if let Some(x) = val.try_downcast_mut::<u64>() {
        let y = u64::from_str(arg);
        if y.is_ok() {
            *x = y.unwrap();
            return true;
        }
        return false;
    } else if let Some(x) = val.try_downcast_mut::<bool>() {
        *x = match arg.to_lowercase().as_str() {
            "true" | "t" | "yes" | "y" => true,
            "false" | "f" | "no" | "n" => false,
            _ => return false,
        };
        return true;
    } else if let Some(x) = val.try_downcast_mut::<String>() {
        *x = arg.clone();
        return true;
    }
    false
}

enum InterCmd {
    Print,
    Set,
    Load,
    Parse,
    Step,
    Exit,
}

fn parse_command(text: &String) -> Option<(InterCmd, Option<String>)> {
    let mut arg = String::new();
    let cmd = match text.split_once(" ") {
        Some(res) => {
            arg = String::from(res.1.trim());
            res.0
        }
        None => text.trim(),
    };
    if cmd == "exit" {
        return Some((InterCmd::Exit, None));
    } else if cmd == "print" {
        return Some((InterCmd::Print, Some(arg)));
    } else if cmd == "parse" {
        return Some((InterCmd::Parse, Some(arg)));
    } else if cmd == "load" {
        return Some((InterCmd::Load, Some(arg)));
    } else if cmd == "step" {
        return Some((InterCmd::Step, Some(arg)));
    } else if cmd == "set" {
        return Some((InterCmd::Set, Some(arg)));
    }
    return None;
}

fn open_output(path: &String) -> Box<dyn Write> {
    if path == "-" {
        Box::new(io::stdout())
    } else {
        Box::new(File::create(path).expect("Bad output file"))
    }
}

fn file_from_path(filename: &str) -> String {
    // Get string version of path
    let path_str = match env::var("PATH") {
        Ok(val) => val,
        Err(e) => panic!("Failed to fetch PATH from environment"),
    };
    // Split into array of search directories
    // Seperated by semicolons on windows, and colons on linux/macos
    let mut dir_char = '/';
    let paths = if env::consts::OS == "windows" {
        dir_char = '\\';
        path_str.split(';').collect::<Vec<&str>>()
    } else {
        path_str.split(':').collect::<Vec<&str>>()
    };
    for dir in paths {
        // Get full path, adding / if needed
        let full_path = if dir.ends_with(dir_char) {
            String::from(dir) + filename
        } else {
            let mut tmp = String::from(dir);
            tmp.push(dir_char);
            tmp + filename
        };
        if fs::exists(&full_path).unwrap_or(false) {
            return full_path;
        }
    }
    panic!("No such input file");
}

fn handle_args() -> Option<Arguments> {
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
                "The architecture size (32 or 64 bit)",
                "What version of the x86 architecture the code is written for. Can be either 32, or 64 bit.",
                ArgValue::Text(String::from("x64")),
                vec!["-a", "--arch"],
            ),
            Argument::new(
                "input",
                "The input file",
                "Where the data to be decoded comes from, either a valid relative/absolue path or the name of a file on your $PATH.",
                ArgValue::Text(String::from("")),
                vec!["-i", "--input"],
            ),
            Argument::new(
                "interactive",
                "Activate interactive mode",
                // TODO: Detailed help message on interactive commands
                "Run the program in an interactive mode. In interactive mode the user directly instructs the program, and can modify options on the fly. Similar to nslookup's interactive mode (Currently not implemented, sorry).",
                ArgValue::Bool(false),
                vec!["-I", "--inter", "--interactive"],
            ),
            Argument::new(
                "offset",
                "The number of bytes to ignore before parsing",
                "The number of bytes that will be ignored and not loaded to be parsed into instructions. For files with multiple executable sections this value will be ignored, unless no-infer is set.",
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
        return None;
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
                        return None;
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
                        return None;
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
    Some(opts)
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
    let mut test_bytes = vec![0x0f, 0x20, 0b11000000, 0];
    let format = InstructionFormatting {
        ..Default::default()
    };
    let mut dec = Decoder {
        context: Context {
            ..Default::default()
        },
        format: format.clone(),
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
        println!("{}", rep.custom_format(&format));
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

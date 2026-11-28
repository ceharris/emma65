//! `emma6k-tracer`: decodes a binary trace file recorded by `emma6k` (or the
//! `emma65`/`emma09` wrappers, or the debugger) into a disassembly listing,
//! for whichever CPU family the file's header names.

mod columns;
mod mc6809;
mod mos6502;

use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
use std::process::ExitCode;

use clap::Parser;
use emma6k_core::bus::symbol::load_vice_labels;
use emma6k_core::trace::{HEADER_LEN, TraceHeader};
use emma6k_core::{ExpandedPathBuf, SymbolTable};

/// Command-line arguments for `emma6k-tracer`.
#[derive(Parser)]
#[clap(
    name = "emma6k-tracer",
    version = env!("BUILD_VERSION"),
    disable_version_flag = true
)]
struct Args {
    /// Print version information and exit
    #[clap(short = 'v', long = "version", action = clap::ArgAction::Version)]
    version: Option<bool>,

    /// Path to the trace file to read. Reads from stdin if omitted.
    input: Option<ExpandedPathBuf>,
    /// Path to write decoded output to. Writes to stdout if omitted.
    #[clap(long)]
    output: Option<ExpandedPathBuf>,
    /// Path to a VICE-format label file to load into the symbol table. May be repeated.
    #[clap(long = "symbol-file")]
    symbol_files: Vec<ExpandedPathBuf>,
    /// Include bus read/write details for each instruction.
    #[clap(long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = Args::parse();

    let mut symbols = SymbolTable::new();
    for path in &args.symbol_files {
        match load_vice_labels(path).await {
            Ok(table) => symbols.insert_from(&table),
            Err(e) => {
                eprintln!("error: failed to load symbol file {path}: {e}");
                return ExitCode::FAILURE;
            }
        }
    }

    let input: Box<dyn Read> = match &args.input {
        Some(path) => match File::open(path) {
            Ok(f) => Box::new(f),
            Err(e) => {
                eprintln!("error: failed to open input file {path}: {e}");
                return ExitCode::FAILURE;
            }
        },
        None => Box::new(io::stdin()),
    };

    let mut output: Box<dyn Write> = match &args.output {
        Some(path) => match File::create(path) {
            Ok(f) => Box::new(BufWriter::new(f)),
            Err(e) => {
                eprintln!("error: failed to create output file {path}: {e}");
                return ExitCode::FAILURE;
            }
        },
        None => Box::new(BufWriter::new(io::stdout())),
    };

    match decode(input, symbols, args.verbose, &mut *output).and_then(|()| output.flush()) {
        Ok(()) => ExitCode::SUCCESS,
        // The reader of the output went away (e.g. `| head`): not an error.
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Reads the header to learn the CPU family, then decodes the whole file
/// (header included) with that family's reader.
fn decode(
    mut input: impl Read,
    symbols: SymbolTable,
    verbose: bool,
    out: &mut dyn Write,
) -> io::Result<()> {
    let mut header = [0u8; HEADER_LEN];
    input.read_exact(&mut header)?;
    let family = TraceHeader::parse(&header)?.family;
    // Put the header back in front of the records, for the family's reader.
    let input = io::Cursor::new(header).chain(input);
    match family {
        emma65::emulator::cpu::trace::FAMILY => mos6502::run(input, symbols, verbose, out),
        emma09::cpu::trace::FAMILY => mc6809::run(input, symbols, verbose, out),
        other => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("the trace is for an unknown CPU family ({other})"),
        )),
    }
}

#![crate_name = "split"]

/*
 * This file is part of the uutils coreutils package.
 *
 * (c) Akira Hayakawa <ruby.wktk@gmail.com>
 *
 * For the full copyright and license information, please view the LICENSE
 * file that was distributed with this source code.
 */

#![feature(macro_rules)]

extern crate getopts;
extern crate libc;

use std::io;
use std::num::Int;
use std::char;

#[path = "../common/util.rs"]
mod util;

static NAME: &'static str = "split";
static VERSION: &'static str = "1.0.0";

pub fn uumain(args: Vec<String>) -> int {
    let opts = [
        getopts::optopt("a", "suffix-length", "use suffixes of length N (default 2)", "N"),
        getopts::optopt("b", "bytes", "put SIZE bytes per output file", "SIZE"),
        getopts::optopt("C", "line-bytes", "put at most SIZE bytes of lines per output file", "SIZE"),
        getopts::optflag("d", "numeric-suffixes", "use numeric suffixes instead of alphabetic"),
        getopts::optopt("l", "lines", "put NUMBER lines per output file", "NUMBER"),
        getopts::optflag("", "verbose", "print a diagnostic just before each output file is opened"),
        getopts::optflag("h", "help", "display help and exit"),
        getopts::optflag("V", "version", "output version information and exit"),
    ];

    let matches = match getopts::getopts(args.tail(), &opts) {
        Ok(m) => m,
        Err(f) => crash!(1, "{}", f)
    };

    if matches.opt_present("h") {
        println!("{} v{}", NAME, VERSION);
        println!("");
        println!("Usage:");
        println!("  {0} [OPTION]... [INPUT [PREFIX]]", NAME);
        println!("");
        io::print(getopts::usage("Output fixed-size pieces of INPUT to PREFIXaa, PREFIX ab, ...; default size is 1000, and default PREFIX is 'x'. With no INPUT, or when INPUT is -, read standard input." , &opts).as_slice());
        println!("");
        println!("SIZE may have a multiplier suffix: b for 512, k for 1K, m for 1 Meg.");
        return 0;
    }

    if matches.opt_present("V") {
        println!("{} v{}", NAME, VERSION);
        return 0;
    }

    let mut settings = Settings {
        prefix: "".to_string(),
        numeric_suffix: false,
        suffix_length: 0,
        input: "".to_string(),
        strategy: "".to_string(),
        strategy_param: "".to_string(),
        verbose: false,
    };

    settings.numeric_suffix = if matches.opt_present("d") { true } else { false };

    settings.suffix_length = match matches.opt_str("a") {
        Some(n) => match n.as_slice().parse() {
            Some(m) => m,
            None => crash!(1, "cannot parse num")
        },
        None => 2
    };

    settings.verbose = if matches.opt_present("verbose") { true } else { false };

    settings.strategy = "l".to_string();
    settings.strategy_param = "1000".to_string();
    let strategies = vec!["b", "C", "l"];
    for e in strategies.iter() {
        match matches.opt_str(*e) {
            Some(a) => {
                if settings.strategy.as_slice() == "l" {
                    settings.strategy = e.to_string();
                    settings.strategy_param = a;
                } else {
                    crash!(1, "{}: cannot split in more than one way", NAME)
                }
            },
            None => {}
        }
    }

    let mut v = matches.free.iter();
    let (input, prefix) = match (v.next(), v.next()) {
        (Some(a), None) => (a.to_string(), "x".to_string()),
        (Some(a), Some(b)) => (a.to_string(), b.to_string()),
        (None, _) => ("-".to_string(), "x".to_string()),
    };
    settings.input = input;
    settings.prefix = prefix;

    split(&settings)
}

struct Settings {
    prefix: String,
    numeric_suffix: bool,
    suffix_length: uint,
    input: String,
    strategy: String,
    strategy_param: String,
    verbose: bool,
}

struct SplitControl {
    current_line: String, // Don't touch
    request_new_file: bool, // Splitter implementation requests new file
}

trait Splitter {
    // Factory pattern
    fn new(_hint: Option<Self>, &Settings) -> Box<Splitter>;

    // Consume the current_line and return the consumed string
    fn consume(&mut self, &mut SplitControl) -> String;
}

struct LineSplitter {
    saved_lines_to_write: uint,
    lines_to_write: uint,
}

impl Splitter for LineSplitter {
    fn new(_: Option<LineSplitter>, settings: &Settings) -> Box<Splitter> {
        let n = match settings.strategy_param.as_slice().parse() {
            Some(a) => a,
            _ => crash!(1, "invalid number of lines")
        };
        box LineSplitter {
            saved_lines_to_write: n,
            lines_to_write: n,
        } as Box<Splitter>
    }

    fn consume(&mut self, control: &mut SplitControl) -> String {
        self.lines_to_write -= 1;
        if self.lines_to_write == 0 {
            self.lines_to_write = self.saved_lines_to_write;
            control.request_new_file = true;
        }
        control.current_line.clone()
    }
}

struct ByteSplitter {
    saved_bytes_to_write: uint,
    bytes_to_write: uint,
    break_on_line_end: bool,
    require_whole_line: bool,
}

impl Splitter for ByteSplitter {
    fn new(_: Option<ByteSplitter>, settings: &Settings) -> Box<Splitter> {
        let mut strategy_param : Vec<char> = settings.strategy_param.chars().collect();
        let suffix = strategy_param.pop().unwrap();
        let multiplier = match suffix {
            '0'...'9' => 1u,
            'b' => 512u,
            'k' => 1024u,
            'm' => 1024u * 1024u,
            _ => crash!(1, "invalid number of bytes")
        };
        let n = if suffix.is_alphabetic() {
            match String::from_chars(strategy_param.as_slice()).as_slice().parse::<uint>() {
                Some(a) => a,
                _ => crash!(1, "invalid number of bytes")
            }
        } else {
            match settings.strategy_param.as_slice().parse::<uint>() {
                Some(a) => a,
                _ => crash!(1, "invalid number of bytes")
            }
        };
        box ByteSplitter {
            saved_bytes_to_write: n * multiplier,
            bytes_to_write: n * multiplier,
            break_on_line_end: if settings.strategy == "b" { false } else { true },
            require_whole_line: false,
        } as Box<Splitter>
    }

    fn consume(&mut self, control: &mut SplitControl) -> String {
        let line = control.current_line.clone();
        let n = std::cmp::min(line.as_slice().chars().count(), self.bytes_to_write);
        if self.require_whole_line && n < line.as_slice().chars().count() {
            self.bytes_to_write = self.saved_bytes_to_write;
            control.request_new_file = true;
            self.require_whole_line = false;
            return line.as_slice().slice(0, 0).to_string();
        }
        self.bytes_to_write -= n;
        if n == 0 {
            self.bytes_to_write = self.saved_bytes_to_write;
            control.request_new_file = true;
        }
        if self.break_on_line_end && n == line.as_slice().chars().count() {
            self.require_whole_line = self.break_on_line_end;
        }
        line.as_slice().slice(0, n).to_string()
    }
}

// (1, 3) -> "aab"
fn str_prefix(i: uint, width: uint) -> String {
    let mut c = "".to_string();
    let mut n = i;
    let mut w = width;
    while w > 0 {
        w -= 1;
        let div = Int::pow(26 as uint, w);
        let r = n / div;
        n -= r * div;
        c.push(char::from_u32((r as u32) + 97).unwrap());
    }
    c
}

// (1, 3) -> "001"
fn num_prefix(i: uint, width: uint) -> String {
    let mut c = "".to_string();
    let mut n = i;
    let mut w = width;
    while w > 0 {
        w -= 1;
        let div = Int::pow(10 as uint, w);
        let r = n / div;
        n -= r * div;
        c.push(char::from_digit(r, 10).unwrap());
    }
    c
}

fn split(settings: &Settings) -> int {
    let mut reader = io::BufferedReader::new(
        if settings.input.as_slice() == "-" {
            box io::stdio::stdin_raw() as Box<Reader>
        } else {
            let r = match io::File::open(&Path::new(settings.input.clone())) {
                Ok(a) => a,
                Err(_) => crash!(1, "cannot open '{}' for reading: No such file or directory", settings.input)
            };
            box r as Box<Reader>
        }
    );

    let mut splitter: Box<Splitter> =
        match settings.strategy.as_slice() {
            "l" => Splitter::new(None::<LineSplitter>, settings),
            "b" | "C" => Splitter::new(None::<ByteSplitter>, settings),
            a @ _ => crash!(1, "strategy {} not supported", a)
        };

    let mut control = SplitControl {
        current_line: "".to_string(), // Request new line
        request_new_file: true, // Request new file
    };

    let mut writer = io::BufferedWriter::new(box io::stdio::stdout_raw() as Box<Writer>);
    let mut fileno = 0;
    loop {
        if control.current_line.as_slice().chars().count() == 0 {
            match reader.read_line() {
                Ok(a) => { control.current_line = a; }
                Err(_) =>  { break; }
            }
        }

        if control.request_new_file {
            let mut filename = settings.prefix.to_string();
            filename.push_str(if settings.numeric_suffix {
                num_prefix(fileno, settings.suffix_length)
            } else {
                str_prefix(fileno, settings.suffix_length)
            }.as_slice());

            if fileno != 0 {
                crash_if_err!(1, writer.flush());
            }
            fileno += 1;
            writer = io::BufferedWriter::new(box io::File::open_mode(&Path::new(filename.as_slice()), io::Open, io::Write) as Box<Writer>);
            control.request_new_file = false;
            if settings.verbose {
                println!("creating file '{}'", filename);
            }
        }

        let consumed = splitter.consume(&mut control);
        crash_if_err!(1, writer.write_str(consumed.as_slice()));

        let advance = consumed.as_slice().chars().count();
        let clone = control.current_line.clone();
        let sl = clone.as_slice();
        control.current_line = sl.slice(advance, sl.chars().count()).to_string();
    }
    0
}

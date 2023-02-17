extern crate pest;
#[macro_use]
extern crate pest_derive;

use pest::error::Error;
use pest::Parser;

use std::collections::HashMap;
use std::fs;

#[derive(Parser)]
#[grammar = "cdl.pest"]
pub struct CDLParser;

#[derive(Debug, Clone, Copy, Default)]
pub struct Stat {
    transistors: i64,
    count: i64,
}

impl Stat {
    pub fn new(transistors: i64, count: i64) -> Self {
        Self {
            transistors: transistors,
            count: count,
        }
    }
}

type LibCellName = String;
type TransistorTable = HashMap<LibCellName, Stat>;

use std::error::Error as StdError;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::{cmp};
use sv_parser::{parse_sv, unwrap_node, Locate, RefNode, SyntaxTree};
use sv_parser_error;

fn main() {
    let cdl_file = std::env::args().nth(1).expect("No cdl file is given!");
    let verilog_file = std::env::args().nth(2).expect("No cdl file is given!");

    let unparsed_cdl = fs::read_to_string(cdl_file).expect("cannot read file");
    let mut transistors_table = cdl_parse(&unparsed_cdl).unwrap();

    let unparsed_verilog = PathBuf::from(verilog_file);
    v_parse(unparsed_verilog, &mut transistors_table);

    let transistors = transistors_table.iter().map(|x| { x.1.transistors * x.1.count }).sum::<i64>();
    println!("Transistors: {}", transistors);
}

fn cdl_parse(file: &str) -> Result<TransistorTable, Error<Rule>> {
    let cdl = CDLParser::parse(Rule::CDL, &file)?;

    let mut table = TransistorTable::new();

    for pair in cdl {
        match pair.as_rule() {
            Rule::CDL => {
                for cpair in pair.into_inner() {
                    match cpair.as_rule() {
                        Rule::SUBCKT => {
                            let mut name = String::new();
                            let mut transistors = 0;

                            for spair in cpair.into_inner() {
                                match spair.as_rule() {
                                    Rule::CKT => {
                                        name = cdl_get_ckt_name(spair);
                                    }
                                    Rule::CKT_BODY => {
                                        for bpair in spair.into_inner() {
                                            match bpair.as_rule() {
                                                Rule::ELEMENT_M => transistors += 1,
                                                _ => {}
                                            }
                                        }
                                    }
                                    Rule::CKT_END => {
                                        let end_name = cdl_get_ckt_name(spair);
                                        assert_eq!(end_name, name);
                                    }
                                    _ => {}
                                }
                            }

                            table.insert(name, Stat::new(transistors, 0));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    Ok(table)
}

fn cdl_get_ckt_name(pair: pest::iterators::Pair<Rule>) -> String {
    let mut name = String::new();

    for p in pair.into_inner() {
        match p.as_rule() {
            Rule::CKT_NAME => {
                name = String::from(p.as_str());
            }
            _ => {}
        }
    }

    name
}

fn v_parse(file: PathBuf, transistor_table: &mut TransistorTable) {
    let defines = HashMap::new();
    let includes: Vec<PathBuf> = Vec::new();

    match parse_sv(&file, &defines, &includes, true, true) {
        Ok((syntax_tree, _)) => {
            analyze_defs(&syntax_tree, transistor_table);
        }
        Err(x) => {
            match x {
                sv_parser_error::Error::Parse(Some((origin_path, origin_pos))) => {
                    eprintln!("parse failed: {:?}", file);
                    print_parse_error(&origin_path, &origin_pos);
                }
                x => {
                    eprintln!("parse failed: {:?} ({})", file, x);
                    let mut err = x.source();
                    while let Some(x) = err {
                        eprintln!("  Caused by {}", x);
                        err = x.source();
                    }
                }
            }

            return;
        }
    }
}

static CHAR_CR: u8 = 0x0d;
static CHAR_LF: u8 = 0x0a;

fn print_parse_error(origin_path: &PathBuf, origin_pos: &usize) {
    let mut f = File::open(&origin_path).unwrap();
    let mut s = String::new();
    let _ = f.read_to_string(&mut s);

    let mut pos = 0;
    let mut column = 1;
    let mut last_lf = None;
    while pos < s.len() {
        if s.as_bytes()[pos] == CHAR_LF {
            column += 1;
            last_lf = Some(pos);
        }
        pos += 1;

        if *origin_pos == pos {
            let row = if let Some(last_lf) = last_lf {
                pos - last_lf
            } else {
                pos + 1
            };
            let mut next_crlf = pos;
            while next_crlf < s.len() {
                if s.as_bytes()[next_crlf] == CHAR_CR || s.as_bytes()[next_crlf] == CHAR_LF {
                    break;
                }
                next_crlf += 1;
            }

            let column_len = format!("{}", column).len();

            eprint!(" {}:{}:{}\n", origin_path.to_string_lossy(), column, row);

            eprint!("{}|\n", " ".repeat(column_len + 1));

            eprint!("{} |", column);

            let beg = if let Some(last_lf) = last_lf {
                last_lf + 1
            } else {
                0
            };
            eprint!(
                " {}\n",
                String::from_utf8_lossy(&s.as_bytes()[beg..next_crlf])
            );

            eprint!("{}|", " ".repeat(column_len + 1));

            eprint!(
                " {}{}\n",
                " ".repeat(pos - beg),
                "^".repeat(cmp::min(origin_pos + 1, next_crlf) - origin_pos)
            );
        }
    }
}

fn analyze_defs(syntax_tree: &SyntaxTree, transistor_table: &mut TransistorTable) {
    // &SyntaxTree is iterable
    for node in syntax_tree {
        // The type of each node is RefNode
        match node {
            RefNode::ModuleInstantiation(x) => {
                // write the module name
                let id = match unwrap_node!(x, ModuleIdentifier) {
                    None => {
                        continue;
                    }
                    Some(x) => x,
                };
                let id = match get_identifier(id) {
                    None => {
                        continue;
                    }
                    Some(x) => x,
                };
                let id = match syntax_tree.get_str(&id) {
                    None => {
                        continue;
                    }
                    Some(x) => x,
                };

                if let Some(mut x) = transistor_table.get_mut(id) {
                    x.count += 1;
                }
            }
            _ => (),
        }
    }
}

fn get_identifier(node: RefNode) -> Option<Locate> {
    // unwrap_node! can take multiple types
    match unwrap_node!(node, SimpleIdentifier, EscapedIdentifier) {
        Some(RefNode::SimpleIdentifier(x)) => {
            return Some(x.nodes.0);
        }
        Some(RefNode::EscapedIdentifier(x)) => {
            return Some(x.nodes.0);
        }
        _ => None,
    }
}

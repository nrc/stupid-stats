// Copyright 2015 Nicholas Cameron.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(box_syntax)]
#![feature(rustc_private)]

extern crate getopts;
extern crate rustc;
extern crate rustc_driver;
extern crate syntax;

use rustc::session::Session;
use rustc::session::config::{self, Input, ErrorOutputType};
use rustc_driver::{driver, CompilerCalls, Compilation, RustcDefaultCalls};

use syntax::{ast, attr, diagnostics, visit};
use syntax::print::pprust::path_to_string;

use std::path::PathBuf;


// This is the highest level controller of compiler execution. We often want
// some context to remember facts about compilation (e.g., the input file or
// some processed flags), but for this simple example, we don't need anything.
// We need to delegate to RustcDefaultCalls when we want to do what the rust
// compiler would do in certain circumstances. We do this so that we can emit
// some of the same info to Cargo.
struct StupidCalls {
    default_calls: RustcDefaultCalls,
}

impl StupidCalls {
    fn new() -> StupidCalls {
        StupidCalls { default_calls: RustcDefaultCalls }
    }
}

// CompilerCalls is a trait for controlling compilation at the driver level. It
// is basically a set of callbacks to call at various stages of compilation to
// execute custom actions or influence compilation. We are mostly just going to
// do nothing and let compilation continue.
impl<'a> CompilerCalls<'a> for StupidCalls {
    fn early_callback(&mut self,
                      _: &getopts::Matches,
                      _: &config::Options,
                      _: &diagnostics::registry::Registry,
                      _: ErrorOutputType)
                      -> Compilation {
        Compilation::Continue
    }

    fn late_callback(&mut self,
                     m: &getopts::Matches,
                     s: &Session,
                     i: &Input,
                     odir: &Option<PathBuf>,
                     ofile: &Option<PathBuf>)
                     -> Compilation {
        self.default_calls.late_callback(m, s, i, odir, ofile);
        Compilation::Continue
    }

    fn some_input(&mut self, input: Input, input_path: Option<PathBuf>) -> (Input, Option<PathBuf>) {
        (input, input_path)
    }

    fn no_input(&mut self,
                m: &getopts::Matches,
                o: &config::Options,
                odir: &Option<PathBuf>,
                ofile: &Option<PathBuf>,
                r: &diagnostics::registry::Registry)
                -> Option<(Input, Option<PathBuf>)> {
        self.default_calls.no_input(m, o, odir, ofile, r);
        // This is not optimal error handling.
        panic!("No input supplied to stupid-stats");
    }

    // This is the only really interesting implementation. It is a hook to allow
    // us to supply a CompileController, a struct which gives fine grain control
    // over the phases of compilation and gives us an opportunity to hook into
    // compilation with callbacks.
    fn build_controller(&mut self, _: &Session) -> driver::CompileController<'a> {
        // We mostly want to do what rustc does, which is what basic() will return.
        let mut control = driver::CompileController::basic();
        // But we only need the AST, so we can stop compilation after parsing.
        control.after_parse.stop = Compilation::Stop;
        // And when we stop after parsing we'll call this closure.
        // Note that this will give us an AST before macro expansions, which is
        // not usually what you want.
        control.after_parse.callback = box |state| {
            // Which extracts information about the compiled crate...
            let krate = state.krate.unwrap();

            // ...and walks the AST, collecting stats.
            let mut visitor = StupidVisitor::new();
            visit::walk_crate(&mut visitor, krate);

            // And finally prints out the stupid stats that we collected.
            let cratename = match attr::find_crate_name(&krate.attrs) {
                Some(name) => name.to_string(),
                None => String::from("unknown_crate"),
            };
            println!("In crate: {},\n", cratename);
            println!("Found {} uses of `println!`;", visitor.println_count);

            let (common, common_percent, four_percent) = visitor.compute_arg_stats();
            println!("The most common number of arguments is {} ({:.0}% of all functions);",
                     common, common_percent);
            println!("{:.0}% of functions have four or more arguments.", four_percent);
        };

        control
    }
}

// We'll collect our stats by walking the AST. To do that we need a visitor object.
struct StupidVisitor {
    // The count of prinlns.
    println_count: usize,
    // Count of each number of args, e.g., arg_counts[2] is the number of functions
    // with two arguments.
    arg_counts: Vec<usize>,
}

impl StupidVisitor {
    fn new() -> StupidVisitor {
        StupidVisitor {
            println_count: 0,
            arg_counts: vec![],
        }
    }

    // Returns (most common number of args,
    //          % of fns with that number,
    //          % of fns with four or more args).
    fn compute_arg_stats(&self) -> (usize, f64, f64) {
        let mut total = 0;
        let mut four_or_more = 0;
        let mut common = 0;
        let mut common_index = 0;
        for (i, &c) in self.arg_counts.iter().enumerate() {
            total += c;
            if i >= 4 {
                four_or_more += c;
            }
            if c > common {
                common = c;
                common_index = i;
            }
        }

        let common = common as f64;
        let four_or_more = four_or_more as f64;
        let total = total as f64;
        (common_index, 100.0 * common / total, 100.0 * four_or_more / total)
    }

    fn increment_args(&mut self, args: usize) {
        if self.arg_counts.len() <= args {
            self.arg_counts.resize(args + 1, 0);
        }

        self.arg_counts[args] += 1;
    }
}

// visit::Visitor is the generic trait for walking an AST.
impl<'v> visit::Visitor<'v> for StupidVisitor {
    // We found an item, could be a function.
    fn visit_item(&mut self, i: &'v ast::Item) {
        match i.node {
            ast::ItemKind::Fn(ref decl, _, _, _, _, _) => {
                // Record the number of args.
                self.increment_args(decl.inputs.len());
            }
            _ => {}
        }

        // Keep walking.
        visit::walk_item(self, i)
    }

    // We found a macro.
    fn visit_mac(&mut self, mac: &'v ast::Mac) {
        // Find its name and check if it is "println".
        let path = &mac.node.path;
        if path_to_string(path) == "println" {
            self.println_count += 1;
        }

        // Keep walking.
        visit::walk_mac(self, mac)
    }

    // Note that I don't check methods for the number of arguments because I'm lazy.
}

fn main() {
    // Grab the command line arguments.
    let args: Vec<_> = std::env::args().collect();
    // Run the compiler. Yep, that's it.
    rustc_driver::run_compiler(&args, &mut StupidCalls::new());
}

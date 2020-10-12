// Copyright 2015 Nicholas Cameron.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(rustc_private)]

extern crate rustc_driver;
extern crate rustc_interface;
extern crate rustc_span;
extern crate rustc_ast;
extern crate rustc_ast_pretty;
extern crate rustc_attr;

use rustc_driver::{Compilation, Callbacks};
use rustc_interface::{Config, Queries, interface::Compiler};
use rustc_ast::{ast, visit};
use rustc_ast_pretty::pprust;

// This is the highest level controller of compiler execution. We often want
// some context to remember facts about compilation (e.g., the input file or
// some processed flags), but for this simple example, we don't need anything.
// We need to delegate to RustcDefaultCalls when we want to do what the rust
// compiler would do in certain circumstances. We do this so that we can emit
// some of the same info to Cargo.
struct StupidCalls;

// Callbacks is a trait for running code during compilation at the driver level. It
// is basically a set of callbacks to call at various stages of compilation to
// execute custom actions or influence compilation. We are mostly just going to
// do nothing and let compilation continue.
impl Callbacks for StupidCalls {
    // first callback the compiler driver calls
    fn config(&mut self, config: &mut Config) {
        // this prevents the compiler from dropping the expanded AST
        // although it still works without it?
        config.opts.debugging_opts.save_analysis = true;
    }

    // next step once config has been read and all input parsed
    fn after_parsing<'tcx>(
        &mut self,
        _compiler: &Compiler,
        _queries: &'tcx Queries<'tcx>
    ) -> Compilation {
        Compilation::Continue
    }

    // after macro expansion
    fn after_expansion<'tcx>(
        &mut self,
        _compiler: &Compiler,
        _queries: &'tcx Queries<'tcx>
    ) -> Compilation {
        Compilation::Continue
    }

    // This is a hook to allow us to supply a callback called after analysis.
    // We are given access to the compiler and the various queries run by the compiler
    // as `Compiler` and `Queries` respectively. The `after_analysis` stage of the
    // compiler gives us access to a fully compiled crate with all meta data.
    fn after_analysis<'tcx>(
        &mut self,
        _compiler: &Compiler,
        queries: &'tcx Queries<'tcx>,
    ) -> Compilation {
        // `Queries::parse` gives us access to a `Result<Query<Crate>>` which is exactly what
        // our ast `Visitor` needs.
        let krate = queries.parse().expect("no Result<Query<Crate>> found").take();
        // ...and walks the AST, collecting stats.
        let mut visitor = StupidVisitor::new();
        visit::walk_crate(&mut visitor, &krate);
        // And finally prints out the stupid stats that we collected.
        let crate_name = match rustc_attr::find_crate_name(&krate.attrs) {
            Some(name) => name.to_string(),
            None => String::from("unknown_crate"),
        };
        println!("In crate: {},\n", crate_name);
        println!("Found {} uses of `println!`;", visitor.println_count);

        let (common, common_percent, four_percent) = visitor.compute_arg_stats();
        println!(
            "The most common number of arguments is {} ({:.0}% of all functions);",
            common, common_percent
        );
        println!(
            "{:.0}% of functions have four or more arguments.",
            four_percent
        );

        Compilation::Continue
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
        (
            common_index,
            100.0 * common / total,
            100.0 * four_or_more / total,
        )
    }

    fn increment_args(&mut self, args: usize) {
        if self.arg_counts.len() <= args {
            self.arg_counts.resize(args + 1, 0);
        }

        self.arg_counts[args] += 1;
    }
}

// visit::Visitor is the generic trait for walking an AST.
impl<'a> visit::Visitor<'a> for StupidVisitor {
    // We found an item, could be a function.
    fn visit_item(&mut self, i: &ast::Item) {
        if let ast::ItemKind::Fn(_, ref decl, _, _) = i.kind {
            // record the number of args
            self.increment_args(decl.decl.inputs.len());
        }
        // Keep walking.
        visit::walk_item(self, i)
    }

    // We found a macro.
    fn visit_mac(&mut self, mac: &ast::MacCall) {
        // Find its name and check if it is "println".
        let path = &mac.path;
        if pprust::path_to_string(path) == "println" {
            self.println_count += 1;
        }

        // Keep walking.
        visit::walk_mac(self, mac)
    }

    // Note that I don't check methods for the number of arguments because I'm lazy.
}

/// Adds the correct --sysroot option.
fn sys_root() -> Vec<String> {
    let home = option_env!("RUSTUP_HOME");
    let toolchain = option_env!("RUSTUP_TOOLCHAIN");
    let sysroot = format!("{}/toolchains/{}", home.unwrap(), toolchain.unwrap());
    vec!["--sysroot".into(), sysroot]
}

fn main() {
    let _ = rustc_driver::catch_fatal_errors(|| {
        // Grab the command line arguments.
        let args: Vec<_> = std::env::args_os().flat_map(|s| s.into_string()).collect();
        let args2 = args.iter()
            .map(|s| (*s).to_string())
            .chain(sys_root().into_iter())
            .collect::<Vec<_>>();

        rustc_driver::run_compiler(&args2, &mut StupidCalls, None, None)
    }).map_err(|e| println!("{:?}", e));
}

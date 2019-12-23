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
extern crate rustc_interface;
extern crate rustc_codegen_utils;
extern crate rustc_metadata;
extern crate syntax;

use rustc::session::config::{self, ErrorOutputType, Input};
use rustc_driver::{Compilation, Callbacks, RustcDefaultCalls};
use rustc_codegen_utils::codegen_backend::CodegenBackend;
use rustc_interface::{Config, Queries, interface::Compiler};


use syntax::{ast, attr, visit};
use syntax::source_map::FileLoader;
use syntax::print::pprust::path_to_string;

use std::path::{Path, PathBuf};
use std::fs;
use std::io;
use std::env;
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
        StupidCalls {
            default_calls: RustcDefaultCalls,
        }
    }
}

// Callbacks is a trait for running code during compilation at the driver level. It
// is basically a set of callbacks to call at various stages of compilation to
// execute custom actions or influence compilation. We are mostly just going to
// do nothing and let compilation continue.
impl Callbacks for StupidCalls {
    // first callback the compiler driver calls
    fn config(&mut self, _config: &mut Config) { }

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
        let crate_name = match attr::find_crate_name(&krate.attrs) {
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

        Compilation::Stop
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
        if let ast::ItemKind::Fn(ref decl, _, _) = i.kind {
            // record the number of args
            self.increment_args(decl.decl.inputs.len());
        }
        // Keep walking.
        visit::walk_item(self, i)
    }

    // We found a macro.
    fn visit_mac(&mut self, mac: &ast::Mac) {
        // Find its name and check if it is "println".
        let path = &mac.path;
        if path_to_string(path) == "println" {
            self.println_count += 1;
        }

        // Keep walking.
        visit::walk_mac(self, mac)
    }

    // Note that I don't check methods for the number of arguments because I'm lazy.
}

// pub fn dylib_path_envvar() -> &'static str {
//     if cfg!(windows) {
//         "PATH"
//     } else if cfg!(target_os = "macos") {
//         "DYLD_FALLBACK_LIBRARY_PATH"
//     } else {
//         "LD_LIBRARY_PATH"
//     }
// }

// pub fn dylib_path() -> Vec<PathBuf> {
//     match env::var_os(dylib_path_envvar()) {
//         Some(var) => env::split_paths(&var).collect(),
//         None => Vec::new(),
//     }
// }

// fn standard_lib() -> PathBuf {
//     let sys_root = sys_root();
//     println!("sys root {}",sys_root);
//     let sysroot = PathBuf::from(sys_root.trim());
//     let src_path = sysroot.join("lib").join("rustlib").join("src").join("rust");
//     let lock = src_path.join("Cargo.lock");
//     src_path
// }

// fn sys_root() -> String {
//     use std::process::Command;
//     let out = Command::new("rustc")
//         .arg("--print")
//         .arg("sysroot")
//         .output()
//         .expect("rustc --print sysroot failed");
//         
           // let src_path = "RUST_SRC_PATH";
//         println!("VARS VARS {:#?}", std::env::vars_os().map(|(k, v)| format!("{:?}={:?}", k, v)).collect::<Vec<_>>());
//     String::from_utf8_lossy(&out.stdout).into()
// }

// const ARGS: &[&str] = &[
//     "",
//     "./test_it/src/lib.rs",
//     "--edition=2018",
//     "--crate-name", "tester",
//     // "build.rs",
//     // "--json=diagnostic-rendered-ansi",
//     "--crate-type", "bin",
//     "--emit=dep-info,link",
//     "-C", "debuginfo=2",
//     "--cfg", "feature=\"default\"",
//     // "-C", "metadata=1ab1652a47711491",
//     // "-C", "extra-filename=-1ab1652a47711491",
//     "--out-dir", "/home/devinr/aprog/rust/__forks__/stupid-stats/test_it/target/debug/build/",
//     "-C", "incremental=/home/devinr/aprog/rust/__forks__/stupid-stats/test_it/target/debug/incremental",
//     "-L", "dependency=/home/devinr/aprog/rust/__forks__/stupid-stats/test_it/target/debug/deps",
//     "--extern", "rustc_tools_util=/home/devinr/aprog/rust/__forks__/stupid-stats/test_it/target/deps/librustc_tools_util-623d1a4939323e1f.rlib",
//     "--error-format=json",
//     "--sysroot"
// ];

fn main() {
    let _ = rustc_driver::catch_fatal_errors(|| {
        // Grab the command line arguments.
        let args: Vec<_> = std::env::args_os().flat_map(|s| s.into_string()).collect();
        let std_lib = standard_lib();
        println!("{:?}", args);
        println!("{:?}", std_lib);

        // let args = ARGS.iter()
        //     .map(|s| s.to_string())
        //     .chain(Some(sys_root()).into_iter())
        //     .collect::<Vec<_>>();
        // Run the compiler. Yep, that's it.
        rustc_driver::run_compiler(&args, &mut StupidCalls::new(), None, None)
    }).map_err(|e| println!("{:?}", e));
}

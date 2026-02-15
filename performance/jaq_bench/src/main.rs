//! Standalone Rust benchmark for jaq – no Python overhead.
//!
//! Usage: jaq_bench <mode> <data_file> <filter> [iterations]
//!   mode: "text"    — parse JSON, run filter, format results as text (like `all_text`)
//!         "objects" — parse JSON, run filter, collect results as Val objects (like `all`)

#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

use jaq_all::json::Val;
use jaq_all::{data, fmts, load};
use std::fmt::Write;
use std::{env, fs, process};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        eprintln!(
            "Usage: {} <mode> <data_file> <filter> [iterations]",
            args[0]
        );
        eprintln!("  mode: text | objects");
        process::exit(1);
    }

    let mode = &args[1];
    let data_file = &args[2];
    let filter_code = &args[3];
    let iterations: usize = args.get(4).map(|s| s.parse().unwrap()).unwrap_or(1);

    let raw = fs::read(data_file).unwrap_or_else(|e| {
        eprintln!("Failed to read {data_file}: {e}");
        process::exit(1);
    });
    let input = fmts::read::json::parse_single(&raw).unwrap_or_else(|e| {
        eprintln!("Failed to parse JSON: {e}");
        process::exit(1);
    });

    let filter = data::compile(filter_code).unwrap_or_else(|frs| {
        frs.iter()
            .for_each(|fr| eprint!("{}", load::FileReportsDisp::new(fr)));
        process::exit(1);
    });

    let runner = data::Runner::default();

    let run = |f: &mut dyn FnMut(Val)| {
        let inputs = std::iter::once(Ok::<Val, String>(input.clone()));
        data::run(
            &runner,
            &filter,
            Default::default(),
            inputs,
            |e| eprintln!("{e}"),
            |v| {
                let val = v.map_err(|e| eprintln!("Runtime error: {e}"))?;
                f(val);
                Ok(())
            },
        )
        .ok();
    };

    match mode.as_str() {
        "text" => {
            // Mirrors jaq_bindings_text.py: input_text(data).all_text()
            for _ in 0..iterations {
                let mut buf = String::new();
                run(&mut |val| {
                    if !buf.is_empty() {
                        buf.push('\n');
                    }
                    write!(buf, "{val}").unwrap();
                });
                std::hint::black_box(&buf);
            }
        }
        "objects" => {
            // Mirrors jaq_bindings_pyobj.py: input_value(data).all()
            // Collects into Vec<Val> (Rust objects) instead of formatting to text
            for _ in 0..iterations {
                let mut results = Vec::new();
                run(&mut |val| results.push(val));
                std::hint::black_box(&results);
            }
        }
        other => {
            eprintln!("Unknown mode: {other}. Use 'text' or 'objects'");
            process::exit(1);
        }
    }
}

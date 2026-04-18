use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
struct Args {
    #[clap(help = "First WGSL file (Base)")]
    a_path: PathBuf,

    #[clap(help = "Second WGSL file (To be fused into Base)")]
    b_path: PathBuf,
}

fn main() {
    let args = Args::parse();

    let a_src = std::fs::read_to_string(&args.a_path).unwrap_or_else(|e| {
        panic!("Failed to read {}: {}", args.a_path.display(), e);
    });
    let b_src = std::fs::read_to_string(&args.b_path).unwrap_or_else(|e| {
        panic!("Failed to read {}: {}", args.b_path.display(), e);
    });
    let module_a = parser::parse(&a_src);
    let module_b = parser::parse(&b_src);
    let fused_module = fuse::fuse(module_a, module_b);
    let mut out_str = String::new();
    ast::writer::Writer::default()
        .write_module(&mut out_str, &fused_module)
        .unwrap();

    println!("{}", out_str);
}

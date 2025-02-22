use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
struct AppArgs {
    #[clap(short, long, default_value = "compress")]
    output_dir: PathBuf,
    
    #[clap(short, long, num_args = 1..)]
    input_file: Option<Vec<PathBuf>>,
}

fn main() {
    let args = AppArgs::parse();
    println!("{:?}", args.input_file);
}
use std::env;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: stylecop-rs <path>");
        return ExitCode::from(2);
    };
    if args.next().is_some() {
        eprintln!("usage: stylecop-rs <path>");
        return ExitCode::from(2);
    }
    println!("{}", stylecop_rs::report(Path::new(&path)));
    ExitCode::SUCCESS
}

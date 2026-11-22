use std::env;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args_os().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: stylecop-rs <path>");
        return ExitCode::from(2);
    };
    if args.next().is_some() {
        eprintln!("usage: stylecop-rs <path>");
        return ExitCode::from(2);
    }
    let path = Path::new(&path);
    if let Err(error) = path.metadata() {
        eprintln!("{}: {error}", path.display());
        return ExitCode::from(2);
    }
    print!("{}", stylecop_rs::report(path));
    ExitCode::SUCCESS
}

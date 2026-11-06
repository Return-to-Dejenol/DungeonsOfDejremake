fn main() {
    let Some(src) = std::env::args_os().nth(1) else {
        eprintln!("usage: ddj-extract <MORDOR11.ZIP | MORDOR.WIP | game folder>");
        std::process::exit(2);
    };
    let out = ddj_extract::data_dir();
    match ddj_extract::import(src.as_ref(), &out) {
        Ok(files) => println!("imported {} files into {}", files.len(), out.display()),
        Err(e) => {
            eprintln!("import failed: {e}");
            std::process::exit(1);
        }
    }
}

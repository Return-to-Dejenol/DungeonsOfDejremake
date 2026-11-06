//! Save editor, for testing and investigation.
//!
//!   ddj-save show <save>                 every field
//!   ddj-save get  <save> <field>         one field
//!   ddj-save set  <save> <field> <value> change a field (the file is checked before writing)
//!
//! `<save>` is a remake save (`saves/NAME.json`; fields are paths like `stats.0` or
//! `items.2.charges`) or the original's character file `MDATA4.MDR` (fields are
//! `NAME.field`, e.g. `Ann.gold`; `show` lists the field names).

use std::{fs, path::Path, process::exit};

use ddj_rules::{game::Character, mdata4};
use serde_json::Value;

fn fail(msg: impl std::fmt::Display) -> ! {
    eprintln!("{msg}");
    exit(1)
}

fn pointer(field: &str) -> String {
    field.split('.').map(|p| format!("/{p}")).collect()
}

fn json(path: &Path, cmd: &str, rest: &[String]) {
    let text = fs::read_to_string(path).unwrap_or_else(|e| fail(e));
    let mut v: Value = serde_json::from_str(&text).unwrap_or_else(|e| fail(e));
    match (cmd, rest) {
        ("show", []) => println!("{}", serde_json::to_string_pretty(&v).unwrap()),
        ("get", [f]) => println!("{}", v.pointer(&pointer(f)).unwrap_or_else(|| fail(format!("no field {f}")))),
        ("set", [f, val]) => {
            let slot = v.pointer_mut(&pointer(f)).unwrap_or_else(|| fail(format!("no field {f}")));
            *slot = serde_json::from_str(val).unwrap_or_else(|_| Value::String(val.clone()));
            let out = serde_json::to_vec_pretty(&v).unwrap();
            serde_json::from_slice::<Character>(&out).unwrap_or_else(|e| fail(format!("not a valid save after the change: {e}")));
            fs::write(path, out).unwrap_or_else(|e| fail(e));
        }
        _ => fail("usage: ddj-save show|get|set <save> [field] [value]"),
    }
}

fn original(path: &Path, cmd: &str, rest: &[String]) {
    let mut file = mdata4::File::read(path).unwrap_or_else(|e| fail(e));
    match (cmd, rest) {
        ("show", []) => print!("{}", file.show()),
        ("get", [f]) => println!("{}", file.get(f).unwrap_or_else(|e| fail(e))),
        ("set", [f, val]) => {
            file.set(f, val).unwrap_or_else(|e| fail(e));
            file.write(path).unwrap_or_else(|e| fail(e));
        }
        _ => fail("usage: ddj-save show|get|set <save> [field] [value]"),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [cmd, path, rest @ ..] = args.as_slice() else { fail("usage: ddj-save show|get|set <save> [field] [value]") };
    let path = Path::new(path);
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) { json(path, cmd, rest) } else { original(path, cmd, rest) }
}

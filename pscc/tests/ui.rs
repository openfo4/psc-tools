use std::collections::HashMap;
use std::path::{Path, PathBuf};

use pex::{Module, Value};

fn main() {
    let out_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ui");

    let files = enumerate_dir("tests/psc");

    for path in files {
        let file = std::fs::read_to_string(&path).unwrap();
        let mut cmds = Vec::new();

        let mut offset = 0;
        for line in file.lines() {
            offset += line.len();

            if let Some(cmd) = line.strip_prefix(";@") {
                let cmd = parse_command(cmd, offset, &file);
                cmds.push(cmd);
            }

            offset += '\n'.len_utf8();
        }

        let cmd = std::process::Command::new("../target/debug/pscc")
            .arg("--out-dir")
            .arg(&out_dir)
            .arg(&path)
            .output()
            .unwrap();

        if !cmd.status.success() {
            eprintln!("test failed: {}", path.to_string_lossy());
            eprintln!("{}", std::str::from_utf8(&cmd.stderr).unwrap());
            std::process::exit(1);
        }

        for cmd in cmds {
            match cmd {
                Command::Run {
                    params,
                    func,
                    result,
                } => {
                    let pex = out_dir
                        .join(path.file_name().unwrap())
                        .with_extension("pex");

                    if let Err(err) = run_pex(pex, &func, params, result) {
                        eprintln!("test failed: {}", path.to_string_lossy());
                        eprintln!("{}", err);
                        std::process::exit(1);
                    }
                }
            }
        }
    }
}

fn parse_command(cmd: &str, offset: usize, file: &str) -> Command {
    if let Some(s) = cmd.strip_prefix("run:") {
        let (params, result) = parse_inputs(s);
        let func = extract_function_name(&file[offset..]);
        return Command::Run {
            params,
            func: func.to_owned(),
            result,
        };
    }

    todo!("unknown command: {}", cmd);
}

enum Command {
    Run {
        params: HashMap<String, Value>,
        result: Value,
        func: String,
    },
}

fn enumerate_dir(path: impl AsRef<Path>) -> Vec<PathBuf> {
    let mut dirs = vec![path.as_ref().to_owned()];
    let mut files = Vec::new();

    while let Some(path) = dirs.pop() {
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let meta = entry.metadata().unwrap();

            if meta.is_dir() {
                dirs.push(entry.path());
            } else if meta.is_file() {
                files.push(entry.path());
            }
        }
    }

    files
}

fn parse_inputs(s: &str) -> (HashMap<String, Value>, Value) {
    let mid = s.find("=>").unwrap();

    let mut params = HashMap::new();
    for param in s[..mid].split(",") {
        let (name, value) = param.split_once("=").unwrap();
        let name = name.trim().to_owned();
        let value: i32 = value.trim().parse().unwrap();
        params.insert(name, Value::Int(value));
    }

    let result: i32 = s[mid + "=>".len()..].trim().parse().unwrap();
    let result = Value::Int(result);

    (params, result)
}

fn extract_function_name(file: &str) -> &str {
    let Some(pos) = file.find("function") else {
        todo!();
    };

    let name = &file[pos + "function".len()..].trim();
    let end = name.find('(').unwrap();

    &name[..end]
}

fn run_pex<P>(
    pex: P,
    func: &str,
    params: HashMap<String, Value>,
    expect: Value,
) -> Result<(), String>
where
    P: AsRef<Path>,
{
    let data = std::fs::read(&pex).map_err(|err| {
        format!(
            "could not read .pex file from {}: {}",
            pex.as_ref().to_string_lossy(),
            err,
        )
    })?;
    let module = Module::read(&data).map_err(|err| {
        format!(
            "could not parse pex module from file {}: {}",
            pex.as_ref().to_string_lossy(),
            err,
        )
    })?;

    let result = pex_runner::run(&module, func, params);

    if result != expect {
        return Err(format!(
            "test for {func} failed: expected {expect:?} to be returned, got {result:?}"
        ));
    }

    Ok(())
}

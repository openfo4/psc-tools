use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use bstr::BString;
use colored::Colorize;
use pex::{Module, Value};
use pex_runner::Runner;

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

        if cmds.iter().any(|cmd| {
            matches!(
                cmd,
                Command::Run {
                    params: _,
                    result: _,
                    func: _
                }
            )
        }) {
            let mut opts = RunOptions {
                out_dir: out_dir.clone(),
                name: path.file_name().unwrap().to_owned(),
                imports: Vec::new(),
            };

            for cmd in &cmds {
                match cmd {
                    Command::Import { path } => {
                        opts.imports.push(path.clone());
                    }
                    _ => {}
                }
            }

            let runner = RunnerState::new(&opts).unwrap();

            for cmd in cmds {
                match cmd {
                    Command::Run {
                        params,
                        result,
                        func,
                    } => {
                        runner.call(&func, params, result).unwrap();
                    }
                    _ => {}
                }
            }
        }

        let testname = path.strip_prefix("tests/psc").unwrap();
        println!("test {} ... {}", testname.to_string_lossy(), "ok".green());
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

    if let Some(s) = cmd.strip_prefix("import:") {
        return Command::Import {
            path: s.trim().to_owned(),
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
    Import {
        path: String,
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
        if param.trim().is_empty() {
            continue;
        }

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

struct RunOptions {
    /// Directory where all `.pex` files are placed.
    out_dir: PathBuf,
    /// The name of the file.
    name: OsString,
    imports: Vec<String>,
}

struct RunnerState {
    runner: Runner,
    class: BString,
}

impl RunnerState {
    fn new(opts: &RunOptions) -> Result<Self, String> {
        let mut paths = vec![opts.out_dir.join(&opts.name).with_extension("pex")];
        for name in &opts.imports {
            let path = opts.out_dir.join(name).with_extension("pex");
            paths.push(path);
        }

        let mut class = None;

        let mut runner = Runner::new();
        for (index, path) in paths.iter().enumerate() {
            let data = std::fs::read(&path).map_err(|err| {
                format!(
                    "could not read .pex file from {}: {}",
                    path.to_string_lossy(),
                    err,
                )
            })?;

            let module = Module::read(&data).map_err(|err| {
                format!(
                    "could not parse pex module from file {}: {}",
                    path.to_string_lossy(),
                    err,
                )
            })?;

            if index == 0 {
                class = module
                    .classes
                    .iter()
                    .next()
                    .map(|class| module.strings.get(class.name).unwrap().to_owned());
            }

            runner.register(module);
        }

        let class = class.unwrap();

        Ok(Self { runner, class })
    }

    fn call(
        &self,
        func: &str,
        params: HashMap<String, Value>,
        expect: Value,
    ) -> Result<(), String> {
        let result = self.runner.call(
            self.class.as_ref(),
            func.into(),
            params.into_values().collect(),
        );

        if result != expect {
            return Err(format!(
                "test for {func} failed: expected {expect:?} to be returned, got {result:?}"
            ));
        }

        Ok(())
    }
}

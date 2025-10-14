use std::{
    env, fs,
    io::{self, Write},
    process::{Command, Stdio},
};
fn get_shell_input() -> Vec<String> {
    print!("[QUASH]$ ");
    io::stdout().flush().unwrap();
    let mut buffer = String::new();
    let stdin = io::stdin();
    stdin.read_line(&mut buffer).unwrap();
    let mut parts: Vec<String> = Vec::new();
    let mut quoted = false;
    let mut cur = String::new();
    let mut endedcur = false;
    for i in buffer.chars() {
        match i {
            '"' => {
                if quoted {
                    quoted = false;
                    parts.push(cur);
                    cur = String::new();
                    endedcur = true;
                } else {
                    quoted = true;
                }
            }
            ' ' => {
                if quoted {
                    cur.push(i);
                } else if !endedcur {
                    parts.push(cur);
                    cur = String::new();
                } else {
                    endedcur = false;
                }
            }
            '\n' => {
                parts.push(cur);

                break;
            }
            _ => {
                cur.push(i);
            }
        }
    }

    parts
}
#[inline(always)]
fn files_in_folder(path: &str) -> fs::ReadDir {
    fs::read_dir(path).unwrap()
}
fn substatue(st: String) -> String {
    let mut s = st;

    if !s.contains("$") {
        s
    } else if !s.contains("/") {
        s.remove(s.find("$").unwrap()).to_string();
        env::var_os(s).unwrap().into_string().unwrap()
    } else {
        let g = s.split("/");
        let h: Vec<_> = g.map(|i| substatue(i.to_string())).collect();
        h.join("/")
    }
}
fn command_run(command: Vec<String>) {
    let mut tmp = command.into_iter();
    let i = tmp.next().unwrap();
    let g = i.clone();
    match g.as_str() {
        "exit" | "quit" => {
            std::process::exit(0);
        }
        "echo" => {
            for j in tmp {
                print!("{} ", substatue(j))
            }
            println!();
        }
        "export" => {
            let j = tmp.next();
            if let Some(j) = j {
                // split on the equals sign
                let mut split1 = j.split("=");
                unsafe { env::set_var(split1.next().unwrap(), split1.next().unwrap()) };
            }
        }
        "cd" => {
            let var = tmp.next();
            if let Some(var) = var {
                let path = substatue(var);
                env::set_current_dir(path).unwrap();
                unsafe {
                    env::set_var("PWD", env::current_dir().unwrap().to_str().unwrap());
                };
            }
        }
        "pwd" => {
            println!("{}", env::current_dir().unwrap().to_str().unwrap());
        }
        _ => {
            let name = g;
            let binding = env::var_os("PATH").unwrap();
            let paths = env::split_paths(&binding);
            for path in paths {
                for file in files_in_folder(path.clone().to_str().unwrap()) {
                    if *file.as_ref().unwrap().file_name() == *name {
                        Command::new(file.unwrap().path())
                            .args(tmp.clone().map(substatue))
                            .spawn()
                            .unwrap()
                            .wait()
                            .unwrap();
                        return;
                    }
                }
            }
        }
    }
}
fn main() {
    loop {
        command_run(get_shell_input());
    }
}

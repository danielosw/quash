use std::{
    env, fs,
    io::{self, Read, Write},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread, vec,
};
#[derive(Clone)]
struct Job {
    id: i64,
    command: Vec<String>,
    finished: Arc<Mutex<bool>>,
    pid: u32,
}
struct JobHandler {
    jobs: Vec<Arc<Mutex<Job>>>,
    id: i64,
}
impl Job {
    fn spawn_job(mut self) {
        let g = self.command.clone()[0].clone();
        let mut tmp = self.command.clone();
        tmp.remove(0);
        let mut job = make_procces_job(tmp.into_iter(), g);
        self.pid = job.id();

        // shoot to another thread
        thread::spawn(move || {
            job.wait().unwrap();
            *self.finished.lock().unwrap() = true;
            println!("Job {} done", self.id);
            // reset the console
            //
            print!("[QUASH]$ ");
            io::stdout().flush().unwrap();
        });
    }
}
impl JobHandler {
    fn get_job_by_id(&self, id: i64) -> std::option::Option<std::sync::MutexGuard<'_, Job>> {
        let jobs = self.jobs.iter().clone();
        for g in jobs {
            let i = g.lock().unwrap();
            if i.id == id {
                return Some(i);
            }
        }
        None
    }
    fn get_index_by_id(&self, id: i64) -> Option<i64> {
        let mut counter = 0;
        for g in self.jobs.iter().clone() {
            let i = g.lock().unwrap();
            if i.id == id {
                return Some(counter);
            } else {
                counter += 1;
            }
        }
        None
    }
    fn get_pid_by_id(&self, id: i64) -> u32 {
        self.get_job_by_id(id).unwrap().pid
    }
    fn get_id_by_index(&self, index: usize) -> i64 {
        self.jobs[index].lock().unwrap().id
    }
    fn create_job(&mut self, command: Vec<String>) -> i64 {
        // create the new job
        let new_job = Job {
            id: self.id,
            command,
            finished: Arc::new(Mutex::new(false)),
            pid: 0,
        };
        self.id += 1;
        let id = new_job.id;
        self.jobs
            .insert(self.jobs.len(), Arc::new(Mutex::new(new_job)));
        id
    }
    fn start_job(&self, id: i64) {
        let job_index = self.get_index_by_id(id);
        self.jobs[<i64 as TryInto<usize>>::try_into(job_index.unwrap()).unwrap()]
            .lock()
            .unwrap()
            .clone()
            .spawn_job();
    }
    fn list_jobs(&self) {
        for g in self
            .jobs
            .clone()
            .iter()
            .filter(|x| !*x.lock().unwrap().finished.lock().unwrap())
        {
            let i = g.lock().unwrap();
            println!("[{}] {} {}", i.clone().id, i.pid, i.command.join(" "));
        }
    }
}
fn get_shell_input() -> String {
    print!("[QUASH]$ ");
    io::stdout().flush().unwrap();
    let mut buffer = String::new();
    let stdin = io::stdin();
    stdin.read_line(&mut buffer).unwrap();
    buffer
}

fn process_shell(buffer: String) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut quoted = false;
    let mut singlequoted = false;
    let mut doublequoted = false;
    let mut cur = String::new();
    let mut endedcur = false;
    let mut piping = false;
    for i in buffer.chars() {
        if !piping {
            match i {
                '"' => {
                    if singlequoted {
                        cur.push(i);
                    } else if doublequoted {
                        quoted = false;
                        doublequoted = false;
                        parts.push(cur);
                        cur = String::new();
                        endedcur = true;
                    } else {
                        quoted = true;
                        doublequoted = true;
                    }
                }
                ' ' => {
                    if quoted {
                        cur.push(i);
                    } else if !endedcur && !cur.clone().is_empty() {
                        parts.push(cur);
                        cur = String::new();
                    } else {
                        endedcur = false;
                    }
                }
                '\n' => {
                    if !cur.is_empty() {
                        parts.push(cur.clone());
                    }
                    break;
                }
                '#' => {
                    if quoted {
                        cur.push(i);
                    } else {
                        break;
                    }
                }
                '\'' => {
                    if doublequoted {
                        cur.push(i)
                    } else if singlequoted {
                        singlequoted = false;
                        quoted = false;
                        parts.push(cur);
                        cur = String::new();
                        endedcur = true;
                    } else {
                        singlequoted = true;
                        quoted = true;
                    }
                }
                '|' => {
                    if !quoted {
                        //if we have a pipe we make the rest one big command
                        // if we are in a cur push it so pipes dont break
                        if !cur.clone().is_empty() {
                            parts.push(cur);
                            cur = String::new();
                        }
                        cur.push(i);
                        parts.push(cur);
                        cur = String::new();
                        piping = true;
                        // prevent a space from being consumed
                        endedcur = true;
                    } else {
                        cur.push(i);
                    }
                }
                '&' => {
                    if !quoted {
                        //we are at the end and this is a job
                        if cur.is_empty() {
                            cur.push(i);
                            parts.push(cur.clone());

                            cur = String::new();
                        } else {
                            parts.push(cur);
                            cur = "&".to_string();
                            parts.push(cur.clone());
                            cur = String::new();
                        }
                    }
                }
                _ => {
                    cur.push(i);
                }
            }
        } else {
            match i != '\n' {
                true => {
                    cur.push(i);
                }
                false => {
                    parts.push(cur.clone());
                    break;
                }
            }
        }
    }
    // if their is nothing in parts but something in cur push it to parts
    if parts.is_empty() && !cur.is_empty() {
        parts.push(cur);
    }
    parts
}
#[inline(always)]
fn files_in_folder(path: &str) -> Option<fs::ReadDir> {
    fs::read_dir(path).ok()
}
fn run_proccess(tmp: vec::IntoIter<String>, g: String, pipe: bool, stdiner: String) {
    let name = g;
    let binding = env::var_os("PATH").unwrap();
    let paths = env::split_paths(&binding);
    for path in paths {
        let resulting = files_in_folder(path.clone().to_str().unwrap());
        if let Some(resulting) = resulting {
            for file in resulting {
                if *file.as_ref().unwrap().file_name() == *name {
                    // check if file is a executable
                    if !file.as_ref().unwrap().metadata().unwrap().is_dir() {
                        // finally are we piping
                        if !pipe {
                            Command::new(file.unwrap().path())
                                .args(tmp.clone().map(substatue))
                                .spawn()
                                .unwrap()
                                .wait()
                                .unwrap();
                        } else {
                            let mut process = Command::new(file.unwrap().path())
                                .args(tmp.clone().map(substatue))
                                .stdin(Stdio::piped())
                                .spawn()
                                .unwrap();
                            process
                                .stdin
                                .as_ref()
                                .unwrap()
                                .write_all(stdiner.as_bytes())
                                .unwrap();
                            process.wait().unwrap();
                        }
                        return;
                    }
                }
            }
        }
    }
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
fn make_procces_job(tmp: vec::IntoIter<String>, g: String) -> std::process::Child {
    let name = g;
    let binding = env::var_os("PATH").unwrap();
    let paths = env::split_paths(&binding);
    for path in paths {
        let resulting = files_in_folder(path.clone().to_str().unwrap());
        if let Some(resulting) = resulting {
            for file in resulting {
                if *file.as_ref().unwrap().file_name() == *name {
                    // check if file is a executable
                    if !file.as_ref().unwrap().metadata().unwrap().is_dir() {
                        let process = Command::new(file.unwrap().path())
                            .args(tmp.clone().map(substatue))
                            .spawn()
                            .unwrap();

                        return process;
                    }
                }
            }
        }
    }

    unreachable!("failed to make process")
}
fn command_pipe_handler(tmp: vec::IntoIter<String>, g: String) -> String {
    let name = g;
    let binding = env::var_os("PATH").unwrap();
    let paths = env::split_paths(&binding);
    let mut buffer = String::new();
    for path in paths {
        let resulting = files_in_folder(path.clone().to_str().unwrap());
        if let Some(resulting) = resulting {
            for file in resulting {
                if *file.as_ref().unwrap().file_name() == *name {
                    // check if file is a executable
                    if !file.as_ref().unwrap().metadata().unwrap().is_dir() {
                        // finally are we piping
                        let mut process = Command::new(file.unwrap().path())
                            .args(tmp.clone().map(substatue))
                            .stdout(Stdio::piped())
                            .spawn()
                            .unwrap();
                        process
                            .stdout
                            .as_mut()
                            .unwrap()
                            .read_to_string(&mut buffer)
                            .unwrap();
                        process.wait().unwrap();
                        return buffer;
                    }
                }
            }
        }
    }

    buffer
}

fn command_run(command: Vec<String>, job_handler: &mut JobHandler) {
    let mut tmp: vec::IntoIter<String> = command.into_iter();
    let tempvec: Vec<String> = tmp.clone().collect();
    let pipe = tempvec.contains(&"|".to_string());
    let job = tempvec.contains(&"&".to_string());
    let i = tmp.next().unwrap();
    let g = i.clone();
    match g.as_str() {
        "exit" | "quit" => {
            std::process::exit(0);
        }
        "echo" => {
            // if thier is no pipe or redirect print the strings
            if !pipe {
                for j in tmp {
                    print!("{} ", substatue(j))
                }
            } else {
                // im doing this synconislly for conviniance

                // create a buffer
                let mut buf = String::new();
                let mut flag = false;
                // until we find a pipe write the strings to the buffer
                for j in tmp {
                    if (j == "|") | flag {
                        // dumb hack
                        if flag {
                            // the only time in this that something actually uses stdin is if its a program, so I am making that assumtion
                            let mut pipeto = process_shell(j);
                            let g2 = pipeto[0].clone();
                            pipeto.remove(0);
                            run_proccess(pipeto.into_iter(), g2, true, buf);
                            return;
                        } else {
                            flag = true;
                        }
                    } else {
                        buf.push_str(&substatue(j));
                    }
                }
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
            if !pipe {
                println!("{}", env::current_dir().unwrap().to_str().unwrap());
            } else {
                let mut flag = false;
                // until we find a pipe write the strings to the buffer
                for j in tmp {
                    if (j == "|") | flag {
                        // dumb hack
                        if flag {
                            // the only time in this that something actually uses stdin is if its a program, so I am making that assumtion
                            let mut pipeto = process_shell(j);
                            let g2 = pipeto[0].clone();
                            pipeto.remove(0);
                            run_proccess(
                                pipeto.into_iter(),
                                g2,
                                true,
                                env::current_dir().unwrap().to_str().unwrap().to_string(),
                            );
                            return;
                        } else {
                            flag = true;
                        }
                    }
                }
            }
        }
        "jobs" => {
            job_handler.list_jobs();
        }
        _ if pipe => {
            // we are piping so
            // get everything up to the pipe
            let mut command: Vec<String> = Vec::new();
            let mut flag = false;
            for j in tmp.as_ref() {
                if j != "|" && !flag {
                    command.push(j.to_owned());
                } else if !flag {
                    flag = true;
                } else {
                    let mut pipeto = process_shell(j.to_owned());
                    let g2 = pipeto[0].clone();

                    pipeto.remove(0);
                    run_proccess(
                        pipeto.into_iter(),
                        g2,
                        true,
                        command_pipe_handler(command.clone().into_iter(), g.clone()),
                    );
                }
            }
        }
        _ if (job) => {
            let mut command: Vec<String> = tmp.collect();
            command.insert(0, g);
            command.remove(command.len() - 1);
            let job = job_handler.create_job(command);
            job_handler.start_job(job);
        }
        _ => {
            run_proccess(tmp, g, false, "".to_string());
        }
    }
}
fn main() {
    let mut job_handler = JobHandler {
        id: 0,
        jobs: Vec::new(),
    };
    loop {
        command_run(process_shell(get_shell_input()), &mut job_handler);
    }
}

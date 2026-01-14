use futures::future::{BoxFuture, FutureExt};
use std::{
    env, fs,
    io::{self, Write},
    process::Stdio,
    sync::{Arc, Mutex},
    vec,
};
use tokio::{
    fs::{read_to_string, File},
    io::{stdout, AsyncReadExt, AsyncWriteExt},
    process::Command as TCommand,
};
#[derive(Clone)]
struct Job {
    id: i64,
    command: Vec<String>,
    finished: Arc<Mutex<bool>>,
    pid: u32,
}
#[derive(Clone)]

struct JobHandler {
    jobs: Vec<Arc<Mutex<Job>>>,
    id: i64,
}
impl Job {
    async fn spawn_job(job_arc: Arc<Mutex<Job>>) {
        let g = job_arc.lock().unwrap().command.clone()[0].clone();
        let mut tmp = job_arc.lock().unwrap().command.clone();
        tmp.remove(0);
        let mut job = make_procces_job(tmp.into_iter(), g).await;
        job_arc.lock().unwrap().pid = job.id().unwrap();
        let binding = job_arc.clone();
        let tmpjob = binding.lock().unwrap();
        println!(
            "Background job started: [{}] {} {}",
            tmpjob.id,
            tmpjob.pid,
            tmpjob.command.join(" ") + " &"
        );
        // use tokio jobs
        tokio::spawn(async move {
            job.wait().await.unwrap();
            *job_arc.lock().unwrap().finished.lock().unwrap() = true;
            let tmpjob = job_arc.lock().unwrap().clone();

            println!(
                "\nCompleted: [{}] {} {}",
                tmpjob.id,
                tmpjob.pid,
                tmpjob.command.join(" ") + " &"
            );
            // reset the console
            print!("[QUASH]$ ");
            stdout().flush().await.unwrap();
        });
    }
}
impl JobHandler {
    async fn get_index_by_id(&self, id: i64) -> Option<i64> {
        let mut counter = 0;
        // cloning here to prevent borrow issues
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

    async fn create_job(&mut self, command: Vec<String>) -> i64 {
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
    async fn start_job(&self, id: i64) {
        let job_index = self.get_index_by_id(id).await;
        let job_arc =
            self.jobs[<i64 as TryInto<usize>>::try_into(job_index.unwrap()).unwrap()].clone();
        Job::spawn_job(job_arc).await;
    }
    async fn list_jobs(&self) {
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
// not async to prevent stdio not properly flushing
/// Gets the shell input from the user and returns it as a String
/// # Example
/// ```
/// let input = get_shell_input();
/// println!("User input: {}", input);
/// ```
fn get_shell_input() -> String {
    print!("[QUASH]$ ");
    io::stdout().flush().unwrap();
    let mut buffer = String::new();
    let stdin = io::stdin();
    stdin.read_line(&mut buffer).unwrap();
    buffer
}

async fn process_shell(buffer: String) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut quoted = false;
    let mut singlequoted = false;
    let mut doublequoted = false;
    let mut cur = String::new();
    let mut endedcur = false;
    let mut piping = false;
    let mut escape = false;
    for i in buffer.chars() {
        if !piping {
            match i {
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
                _ if escape => {
                    cur.push(i);
                    escape = false;
                }
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
                '&' | '>' | '<' => {
                    if !quoted {
                        //we are at the end and this is a job
                        if cur.is_empty() {
                            cur.push(i);
                            parts.push(cur.clone());

                            cur = String::new();
                        } else {
                            parts.push(cur);
                            cur = i.to_string();
                            parts.push(cur.clone());
                            cur = String::new();
                        }
                    }
                }
                '\\' => {
                    escape = true;
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
async fn files_in_folder(path: &str) -> Option<fs::ReadDir> {
    fs::read_dir(path).ok()
}
async fn run_proccess(tmp: vec::IntoIter<String>, g: String, pipe: bool, stdiner: String) {
    let name = g;
    let binding = env::var_os("PATH").unwrap();
    let paths = env::split_paths(&binding);
    for path in paths {
        let resulting = files_in_folder(path.clone().to_str().unwrap()).await;
        if let Some(resulting) = resulting {
            for file in resulting {
                if *file.as_ref().unwrap().file_name() == *name {
                    // check if file is a executable
                    if !file.as_ref().unwrap().metadata().unwrap().is_dir() {
                        // finally are we piping
                        if !pipe {
                            TCommand::new(file.unwrap().path())
                                .args(tmp.clone().map(substatue))
                                .spawn()
                                .unwrap()
                                .wait()
                                .await
                                .unwrap();
                        } else {
                            let mut process = TCommand::new(file.unwrap().path())
                                .args(tmp.clone().map(substatue))
                                .stdin(Stdio::piped())
                                .spawn()
                                .unwrap();
                            process
                                .stdin
                                .as_mut()
                                .unwrap()
                                .write_all(stdiner.as_bytes())
                                .await
                                .unwrap();
                            process.wait().await.unwrap();
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
async fn make_procces_job(tmp: vec::IntoIter<String>, g: String) -> tokio::process::Child {
    let name = g;
    let binding = env::var_os("PATH").unwrap();
    let paths = env::split_paths(&binding);
    for path in paths {
        let resulting = files_in_folder(path.clone().to_str().unwrap()).await;
        if let Some(resulting) = resulting {
            for file in resulting {
                if *file.as_ref().unwrap().file_name() == *name {
                    // check if file is a executable
                    if !file.as_ref().unwrap().metadata().unwrap().is_dir() {
                        let process = TCommand::new(file.unwrap().path())
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
async fn command_pipe_handler(tmp: vec::IntoIter<String>, g: String) -> String {
    let name = g;
    let binding = env::var_os("PATH").unwrap();
    let paths = env::split_paths(&binding);
    let mut buffer = String::new();
    for path in paths {
        let resulting = files_in_folder(path.clone().to_str().unwrap()).await;
        if let Some(resulting) = resulting {
            for file in resulting {
                if *file.as_ref().unwrap().file_name() == *name {
                    // check if file is a executable
                    if !file.as_ref().unwrap().metadata().unwrap().is_dir() {
                        // finally are we piping
                        let mut process = TCommand::new(file.unwrap().path())
                            .args(tmp.clone().map(substatue))
                            .stdout(Stdio::piped())
                            .spawn()
                            .unwrap();
                        process
                            .stdout
                            .as_mut()
                            .unwrap()
                            .read_to_string(&mut buffer)
                            .await
                            .unwrap();
                        process.wait().await.unwrap();
                        return buffer;
                    }
                }
            }
        }
    }

    buffer
}

async fn command_run(
    command: Vec<String>,
    job_handler: &mut Arc<Mutex<JobHandler>>,
    return_string: bool,
    stdin: bool,
    texter: String,
) -> BoxFuture<'static, Option<String>> {
    let mut tmp: vec::IntoIter<String> = command.into_iter();
    let tempvec: Vec<String> = tmp.clone().collect();
    let pipe = tempvec.contains(&"|".to_string());
    let job = tempvec.contains(&"&".to_string());
    let tofile = tempvec.contains(&">".to_string());
    let fromfile = tempvec.contains(&"<".to_string());
    // return if tmp is empty so we don't break
    if tempvec.is_empty() {
        return async move { None }.boxed();
    };
    let i = tmp.next().unwrap();
    let g = i.clone();
    match g.as_str() {
        "exit" | "quit" => {
            std::process::exit(0);
        }
        _ if tofile => {
            let mut command: Vec<String> = Vec::new();
            let mut flag = false;
            for j in tmp.as_ref() {
                if j != ">" && !flag {
                    command.push(j.to_owned());
                } else if !flag {
                    flag = true;
                } else {
                    command.insert(0, g.clone());

                    let text = Box::pin(command_run(
                        command.clone().into_iter().collect(),
                        job_handler,
                        true,
                        false,
                        "".to_string(),
                    ))
                    .await
                    .await;
                    let create_result = File::create(j).await;
                    let mut towrite = match create_result {
                        Ok(file) => file,
                        Err(error) => {
                            println!("Failed to make file: {}", error);
                            return async move { None }.boxed();
                        }
                    };
                    towrite
                        .write_all(text.clone().unwrap().as_bytes())
                        .await
                        .unwrap();
                }
            }
        }
        _ if fromfile => {
            // this is basiclly piping but we read from a file instead of from a command
            let mut command: Vec<String> = Vec::new();
            let mut flag = false;
            for j in tmp.as_ref() {
                if j != "<" && !flag {
                    command.push(j.to_owned());
                } else if !flag {
                    flag = true;
                } else {
                    let create_result = read_to_string(j).await;
                    let read = match create_result {
                        Ok(file) => file,
                        Err(error) => {
                            println!("Failed to read file: {}", error);
                            return async move { None }.boxed();
                        }
                    };
                    command.insert(0, g.clone());

                    Box::pin(command_run(command.clone(), job_handler, false, true, read))
                        .await
                        .await;
                }
            }
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
                    let pipeto = process_shell(j.to_owned()).await;
                    command.insert(0, g.clone());
                    let value = command.clone();
                    let inner_result = Box::pin(command_run(
                        value.clone().into_iter().collect(),
                        job_handler,
                        true,
                        false,
                        "".to_string(),
                    ))
                    .await
                    .await
                    .unwrap();
                    Box::pin(command_run(
                        pipeto,
                        &mut job_handler.clone(),
                        false,
                        true,
                        inner_result,
                    ))
                    .await
                    .await;
                }
            }
        }
        "echo" => {
            // if thier is no pipe or redirect print the strings
            if !return_string {
                println!("{}", substatue(tmp.collect::<Vec<String>>().join(" ")));
            } else {
                return async move { Some(tmp.collect::<Vec<String>>().join(" ")) }.boxed();
            }
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
            if !return_string {
                println!("{}", env::current_dir().unwrap().to_str().unwrap());
            } else {
                return async move {Some(env::current_dir().unwrap().to_str().unwrap().to_string())}.boxed();
            }
        }
        "jobs" => {
            job_handler.lock().unwrap().list_jobs().await;
        }
        /*
               "kill" => {
                   let args: Vec<String> = tmp.collect();
                   unsafe extern "C" {
                       unsafe fn kill(pid: i32, sig: i32) -> i32;
                   }
                   // because this is an external function rust can't gaurrenty memory safety
                   unsafe {
                       kill(args[1].parse().unwrap(), args[0].parse().unwrap());
                   }
               }
        */
        _ if (job) => {
            let mut command: Vec<String> = tmp.collect();
            command.insert(0, g);
            command.remove(command.len() - 1);
            let job = job_handler.lock().unwrap().create_job(command).await;
            job_handler.lock().unwrap().start_job(job).await;
        }
        _ => {
            if !stdin && !return_string {
                run_proccess(tmp, g, false, "".to_string()).await;
            } else if !return_string && stdin {
                run_proccess(tmp, g, true, texter).await;
            } else if return_string && !stdin {
                return async move { Some(command_pipe_handler(tmp, g).await) }.boxed();
            }
        }
    }

    async move { None }.boxed()
}
#[tokio::main]
async fn main() {
    let mut job_handler = Arc::new(Mutex::new(JobHandler {
        id: 1,
        jobs: Vec::new(),
    }));
    loop {
        command_run(
            process_shell(get_shell_input()).await.into_iter().collect(),
            &mut job_handler,
            false,
            false,
            "".to_string(),
        )
        .await
        .await;
    }
}

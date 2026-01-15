use tokio::process::Command as TCommand;

use crate::{files_in_folder, substatue};

use std::collections::HashMap;

use parking_lot::Mutex;
use std::io::Write;
use std::{env, vec};

use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct Job {
    pub(crate) id: i64,
    pub(crate) command: Vec<String>,
    pub(crate) finished: Arc<Mutex<bool>>,
    pub(crate) pid: u32,
}

#[derive(Clone)]

pub(crate) struct JobHandler {
    pub(crate) jobs: HashMap<i64, Arc<Mutex<Job>>>,
    pub(crate) id: i64,
}

impl Job {
    pub(crate) async fn spawn_job(job_arc: Arc<Mutex<Job>>) {
        let g = job_arc.lock().command.clone()[0].clone();
        let mut tmp = job_arc.lock().command.clone();
        tmp.remove(0);
        let mut job = make_procces_job(tmp.into_iter(), g).await;
        job_arc.lock().pid = job.id().unwrap();
        let binding = job_arc.clone();
        let tmpjob = binding.lock();
        println!(
            "Background job started: [{}] {} {}",
            tmpjob.id,
            tmpjob.pid,
            tmpjob.command.join(" ") + " &"
        );
        // use tokio jobs
        tokio::spawn(async move {
            job.wait().await.unwrap();
            *job_arc.lock().finished.lock() = true;
            let tmpjob = job_arc.lock().clone();

            println!(
                "\nCompleted: [{}] {} {}",
                tmpjob.id,
                tmpjob.pid,
                tmpjob.command.join(" ") + " &"
            );
            // reset the console
            print!("[QUASH]$ ");
            // flush stdout to ensure prompt appears
            std::io::stdout().flush().unwrap();
        });
    }
}

impl JobHandler {
    pub(crate) async fn get_index_by_id(&self, id: i64) -> Option<i64> {
        if let Some(_) = self.jobs.get(&id) {
            return Some(id);
        }
        None
    }

    pub(crate) async fn create_job(&mut self, command: Vec<String>) -> i64 {
        // create the new job
        let new_job = Job {
            id: self.id,
            command,
            finished: Arc::new(Mutex::new(false)),
            pid: 0,
        };
        self.id += 1;
        let id = new_job.id;
        self.jobs.insert(
            self.jobs.len().try_into().unwrap(),
            Arc::new(Mutex::new(new_job)),
        );
        id
    }
    pub(crate) async fn start_job(&self, id: i64) {
        let job_index = self.get_index_by_id(id).await;
        let job_arc = self.jobs[&job_index.unwrap()].clone();
        Job::spawn_job(job_arc).await;
    }
    pub(crate) async fn list_jobs(&self) {
        for g in self
            .jobs
            .clone()
            .iter()
            .filter(|x| !*x.1.lock().finished.lock())
        {
            let i = g.1.lock();
            println!("[{}] {} {}", i.clone().id, i.pid, i.command.join(" "));
        }
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

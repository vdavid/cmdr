//! The viewer's jobs run on threads of their own natively: a search or an index scan must
//! never run on the caller's thread, which is a command's IPC or blocking thread.

use std::sync::mpsc;
use std::time::Duration;

use super::worker;

#[test]
fn a_job_runs_on_another_thread_and_can_be_waited_for() {
    let (tx, rx) = mpsc::channel();
    let job = worker::spawn(move || {
        tx.send(std::thread::current().id()).expect("the test is listening");
    });
    job.join();
    let ran_on = rx.recv().expect("the job ran");
    assert_ne!(ran_on, std::thread::current().id());
}

#[test]
fn a_delayed_job_runs_later_on_another_thread() {
    let (tx, rx) = mpsc::channel();
    let started = std::time::Instant::now();
    worker::after(Duration::from_millis(30), move || {
        tx.send((std::thread::current().id(), started.elapsed()))
            .expect("the test is listening");
    });
    let (ran_on, waited) = rx.recv_timeout(Duration::from_secs(5)).expect("the job ran");
    assert_ne!(ran_on, std::thread::current().id());
    assert!(waited >= Duration::from_millis(30), "ran after {waited:?}");
}

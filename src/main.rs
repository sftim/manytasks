use libc;
use anyhow::{Context, Result,anyhow};
use nix::sys::wait::{wait,WaitStatus};
use nix::unistd::fork;
use nix::unistd::ForkResult::{Child, Parent};
use std::cmp;
use std::convert::TryInto;
use std::thread;
use std::time::Duration;
use std::time::Instant;

struct Sleeper(thread::JoinHandle<()>);

fn make_sleeper(duration: Duration) -> Result<Sleeper> {
  const STACK_SIZE: usize = 1 << 10;
  let builder = thread::Builder::new()
                                .name("sleeper".into())
                                .stack_size(STACK_SIZE);

  let sleeper_thread = builder.spawn(move || { thread::sleep(duration) })?;
  Ok(Sleeper(sleeper_thread))
}

fn make_many_sleepers(count: usize, duration: Duration, start_time: Instant) ->  Result<()> {
  let sleep_after_thread_count = 20;
  let sleep_after_thread_duration = Duration::from_millis(20);
  let sleep_after_error_duration = Duration::from_millis(500);

  let mut cached_now = Instant::now();
  const MINIMUM_SLEEP_DURATION: Duration = Duration::from_secs(1);

  let mut tasks_started: usize = 0;
  let mut failures = 0; // resettable; used for backoffs

  let mut sleepers: Vec<Sleeper> = Vec::with_capacity(count);
  while tasks_started < count {
    // work out how long to sleep for, for this task
    let elapsed_time_so_far = cached_now - start_time;
    let this_task_sleep_duration = if duration > elapsed_time_so_far {
      duration - elapsed_time_so_far
    } else {
      MINIMUM_SLEEP_DURATION
    };

    // make the new thread
    match make_sleeper(this_task_sleep_duration) {
       Ok(sleeper) => {
         sleepers.push(sleeper);
         tasks_started += 1;
         failures = 0;
       }
       Err(err) => {
         failures += 1;
         eprintln!("Error making task: {:?}", err);
         thread::sleep((1 + failures ) * sleep_after_error_duration);
         // fetch time after sleep
         cached_now = Instant::now();
       }
    }

    // don't hog all the CPU all the time
    if (tasks_started % sleep_after_thread_count) == 0 {
      thread::sleep(sleep_after_thread_duration);
      // fetch time after sleep
      cached_now = Instant::now();
    }
  }

  // wait for them all
  eprintln!("Waiting for sleeping children");
  for sleeper in sleepers {
    let _ = sleeper.0.join(); // ignore errors here
  }
  Ok(())
}

fn setup<P: Into<u16>> (target_niceness: P) -> Result<()> {

  let target_niceness: u16 = target_niceness.into();

  let current_priority = unsafe {
    errno::set_errno(errno::Errno(0));
    let current_priority = libc::getpriority(libc::PRIO_PROCESS, 0);
    let errno = errno::errno();
    match errno.into() {
       0 => Ok(current_priority),
       _ => Err(errno)
    }
  }.context("Failed to fetch priority")?;

  let set_priority = |priority| -> Result<()> {
    unsafe {
       errno::set_errno(errno::Errno(0));
       match libc::setpriority(libc::PRIO_PROCESS, 0, priority).into() {
         0 => Ok(()),
         _ => Err(errno::errno())
       }
    }.context("Failed to set priority")?;

    Ok(())
  };

  if current_priority <= target_niceness.into() {
    set_priority(target_niceness.try_into()?)?;
  }
  Ok(())
}

fn parse_arguments() -> Result<usize> {
  let mut sleeper_count_arg: Option<usize> = None;
  for raw_argument in std::env::args_os().skip(1) {
    if sleeper_count_arg != None {
      return Err(anyhow!("Too many command line arguments"));
    }
    match raw_argument.into_string() {
        Ok(argument) => sleeper_count_arg = Some(argument.parse().context("Invalid command line argument")?),
        Err(item) => return Err(anyhow!("Invalid argument ({:?})", item))
    }
  }

  // default count
  match sleeper_count_arg {
    None => Ok(1), // default
    Some(value) => Ok(value.into())
  }
}

fn main() -> Result<()> {
    const TARGET_NICENESS: u8 = 15;
    const SLEEP_DURATION: Duration = Duration::from_secs(15);
    const WORKERS_PER_PROCESS: usize = 8192;

    let mut sleeper_count = parse_arguments()?;

    let mut workers = Vec::with_capacity(sleeper_count / WORKERS_PER_PROCESS);

    setup(TARGET_NICENESS)?;

    let start_time = Instant::now();

    eprintln!("Creating {} threads for {} seconds", sleeper_count, SLEEP_DURATION.as_secs());
    // set up n workers
    while sleeper_count > 0 {
      let sleepers_to_make = cmp::min(sleeper_count, WORKERS_PER_PROCESS);
      sleeper_count -= sleepers_to_make;

      let pid = unsafe { fork() }.context("Failed to fork worker process")?;

      match pid {
        Child => {
            make_many_sleepers(sleepers_to_make, SLEEP_DURATION, start_time)?;
            return Ok(());
        },
        Parent { child } => {
          workers.push(child)
        }
      }
    }

    // wait for them all
    while workers.len() > 0 {
      match wait() {
        Err(nix::errno::Errno::ECHILD) => return Ok(()), // no children to worry about
        Ok(WaitStatus::Exited(worker, _)) => { // ignore exit status
          let index = workers.iter().position(|item| *item == worker).context("Unexpected child processs")?;
          workers.remove(index);
        },
        Ok(WaitStatus::Continued(..)) => { }, // ignore
        Ok(WaitStatus::Stopped(..)) => { }, // ignore
        other => return Err(anyhow!("Internal error waiting for worker ({:?})", other))
      }
    }
    Ok(())
}

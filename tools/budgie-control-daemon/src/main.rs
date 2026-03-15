use std::collections::HashSet;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use sysinfo::{Pid, Signal, System};

#[derive(Debug, Clone, Parser)]
#[command(author, version, about)]
struct Args {
    /// CPU percentage hard-limit per process. Processes above this are suspended for one cycle.
    #[arg(long, default_value_t = 85.0)]
    cpu_limit: f32,

    /// Memory limit in MiB per process.
    #[arg(long, default_value_t = 1_024)]
    ram_limit_mib: u64,

    /// Poll interval in milliseconds.
    #[arg(long, default_value_t = 1_000)]
    poll_ms: u64,

    /// Suspend processes with CPU below idle threshold for N cycles.
    #[arg(long, default_value_t = 5.0)]
    idle_cpu_threshold: f32,

    /// Path to write JSON stats for UI consumers.
    #[arg(long, default_value = "/tmp/budgie-control-stats.json")]
    stats_file: PathBuf,

    /// Process name filter.
    #[arg(long, default_value = "budgie-browser")]
    process_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProcessSample {
    pid: usize,
    name: String,
    cpu_percent: f32,
    memory_mib: u64,
    suspended: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DaemonSnapshot {
    generated_at_epoch_ms: u128,
    cpu_limit: f32,
    ram_limit_mib: u64,
    samples: Vec<ProcessSample>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mut system = System::new_all();
    let mut idle_streak: HashSet<Pid> = HashSet::new();

    loop {
        let started = Instant::now();
        system.refresh_all();

        let mut samples = Vec::new();

        for (pid, process) in system.processes() {
            let name_matches = process
                .name()
                .to_ascii_lowercase()
                .contains(&args.process_name.to_ascii_lowercase());
            if !name_matches {
                continue;
            }

            let memory_mib = process.memory() / 1_024;
            let cpu = process.cpu_usage();
            let mut suspended = false;

            if cpu > args.cpu_limit || memory_mib > args.ram_limit_mib {
                let _ = process.kill_with(Signal::Stop);
                suspended = true;
            } else if cpu < args.idle_cpu_threshold {
                if !idle_streak.insert(*pid) {
                    let _ = process.kill_with(Signal::Stop);
                    suspended = true;
                }
            } else if idle_streak.remove(pid) {
                let _ = process.kill_with(Signal::Continue);
            }

            samples.push(ProcessSample {
                pid: pid.as_u32() as usize,
                name: process.name().to_string(),
                cpu_percent: cpu,
                memory_mib,
                suspended,
            });
        }

        if !samples.is_empty() {
            let snapshot = DaemonSnapshot {
                generated_at_epoch_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .context("clock drift while building snapshot")?
                    .as_millis(),
                cpu_limit: args.cpu_limit,
                ram_limit_mib: args.ram_limit_mib,
                samples,
            };
            std::fs::write(&args.stats_file, serde_json::to_vec_pretty(&snapshot)?)
                .with_context(|| format!("failed writing {}", args.stats_file.display()))?;
        }

        let elapsed = started.elapsed();
        let interval = Duration::from_millis(args.poll_ms);
        if elapsed < interval {
            thread::sleep(interval - elapsed);
        }
    }
}

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::{ArgAction, Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use sysinfo::{Pid, Signal, System};
use tiny_http::{Header, Method, Response, Server, StatusCode};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeLimits {
    cpu_limit: f32,
    ram_limit_mib: u64,
    idle_cpu_threshold: f32,
    network_limit_kib_s: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum GxPreset {
    Eco,
    Balanced,
    Beast,
}

impl GxPreset {
    fn limits(&self) -> RuntimeLimits {
        match self {
            Self::Eco => RuntimeLimits {
                cpu_limit: 35.0,
                ram_limit_mib: 1024,
                idle_cpu_threshold: 2.5,
                network_limit_kib_s: 512,
            },
            Self::Balanced => RuntimeLimits {
                cpu_limit: 65.0,
                ram_limit_mib: 3072,
                idle_cpu_threshold: 4.0,
                network_limit_kib_s: 4096,
            },
            Self::Beast => RuntimeLimits {
                cpu_limit: 95.0,
                ram_limit_mib: 8192,
                idle_cpu_threshold: 7.5,
                network_limit_kib_s: 0,
            },
        }
    }
}

#[derive(Debug, Clone, Parser)]
#[command(author, version, about)]
struct Args {
    /// CPU percentage hard-limit per process. Processes above this are suspended for one cycle.
    #[arg(long, default_value_t = 85.0)]
    cpu_limit: f32,

    /// Memory limit in MiB per process.
    #[arg(long, default_value_t = 1_024)]
    ram_limit_mib: u64,

    /// Network limit in KiB/s for future enforcement integrations (0 disables).
    #[arg(long, default_value_t = 0)]
    network_limit_kib_s: u64,

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

    /// Bind address for in-browser control panel/API.
    #[arg(long, default_value = "127.0.0.1:47831")]
    bind: String,

    /// Print per-cycle summary lines.
    #[arg(long, action = ArgAction::Set, default_value_t = true)]
    verbose: bool,

    /// Optional GX preset at startup.
    #[arg(long)]
    preset: Option<GxPreset>,
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
    limits: RuntimeLimits,
    active_preset: Option<GxPreset>,
    process_name_filter: String,
    sample_count: usize,
    suspended_count: usize,
    samples: Vec<ProcessSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LimitsUpdate {
    cpu_limit: Option<f32>,
    ram_limit_mib: Option<u64>,
    idle_cpu_threshold: Option<f32>,
    network_limit_kib_s: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PresetUpdate {
    preset: GxPreset,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct State {
    limits: RuntimeLimits,
    active_preset: Option<GxPreset>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let initial_limits = match &args.preset {
        Some(preset) => preset.limits(),
        None => RuntimeLimits {
            cpu_limit: args.cpu_limit,
            ram_limit_mib: args.ram_limit_mib,
            idle_cpu_threshold: args.idle_cpu_threshold,
            network_limit_kib_s: args.network_limit_kib_s,
        },
    };

    let state = Arc::new(Mutex::new(State {
        limits: initial_limits,
        active_preset: args.preset,
    }));
    let latest_snapshot: Arc<Mutex<Option<DaemonSnapshot>>> = Arc::new(Mutex::new(None));

    let api_state = Arc::clone(&state);
    let api_snapshot = Arc::clone(&latest_snapshot);
    let bind = args.bind.clone();
    thread::spawn(move || {
        if let Err(err) = run_http_server(&bind, api_state, api_snapshot) {
            eprintln!("Budgie Control API server failed: {err}");
        }
    });

    let mut system = System::new_all();
    let mut idle_counts: HashMap<Pid, u8> = HashMap::new();

    if args.verbose {
        println!(
            "Budgie Control running. Open http://{}/ in Budgie Browser to change limits.",
            args.bind
        );
    }

    loop {
        let started = Instant::now();
        system.refresh_all();

        let current_state = {
            state
                .lock()
                .map_err(|_| anyhow::anyhow!("state lock poisoned"))?
                .clone()
        };

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

            if cpu > current_state.limits.cpu_limit
                || memory_mib > current_state.limits.ram_limit_mib
            {
                let _ = process.kill_with(Signal::Stop);
                suspended = true;
                idle_counts.remove(pid);
            } else if cpu < current_state.limits.idle_cpu_threshold {
                let counter = idle_counts.entry(*pid).or_insert(0);
                *counter = counter.saturating_add(1);
                if *counter >= 2 {
                    let _ = process.kill_with(Signal::Stop);
                    suspended = true;
                }
            } else {
                idle_counts.remove(pid);
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

        let suspended_count = samples.iter().filter(|s| s.suspended).count();
        let snapshot = DaemonSnapshot {
            generated_at_epoch_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .context("clock drift while building snapshot")?
                .as_millis(),
            limits: current_state.limits,
            active_preset: current_state.active_preset,
            process_name_filter: args.process_name.clone(),
            sample_count: samples.len(),
            suspended_count,
            samples,
        };

        std::fs::write(&args.stats_file, serde_json::to_vec_pretty(&snapshot)?)
            .with_context(|| format!("failed writing {}", args.stats_file.display()))?;

        {
            let mut guard = latest_snapshot
                .lock()
                .map_err(|_| anyhow::anyhow!("snapshot lock poisoned"))?;
            *guard = Some(snapshot.clone());
        }

        if args.verbose {
            println!(
                "sampled {} '{}' processes | suspended {} | preset {:?} | limits cpu={:.1}% ram={}MiB net={}KiB/s idle<{:.1}%",
                snapshot.sample_count,
                snapshot.process_name_filter,
                snapshot.suspended_count,
                snapshot.active_preset,
                snapshot.limits.cpu_limit,
                snapshot.limits.ram_limit_mib,
                snapshot.limits.network_limit_kib_s,
                snapshot.limits.idle_cpu_threshold
            );
        }

        let elapsed = started.elapsed();
        let interval = Duration::from_millis(args.poll_ms.max(100));
        if elapsed < interval {
            thread::sleep(interval - elapsed);
        }
    }
}

fn run_http_server(
    bind: &str,
    state: Arc<Mutex<State>>,
    latest_snapshot: Arc<Mutex<Option<DaemonSnapshot>>>,
) -> Result<()> {
    let server = Server::http(bind).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("Budgie Control API listening on http://{bind}/");

    for mut request in server.incoming_requests() {
        let path = request.url().to_string();
        let method = request.method().clone();

        match (method, path.as_str()) {
            (Method::Get, "/") => {
                let body = control_panel_html();
                let response = Response::from_string(body)
                    .with_status_code(StatusCode(200))
                    .with_header(content_type("text/html; charset=utf-8"));
                let _ = request.respond(response);
            }
            (Method::Get, "/status") => {
                let snapshot = latest_snapshot
                    .lock()
                    .map_err(|_| anyhow::anyhow!("snapshot lock poisoned"))?
                    .clone();

                let body = serde_json::to_string_pretty(&snapshot)?;
                let response = Response::from_string(body)
                    .with_status_code(StatusCode(200))
                    .with_header(content_type("application/json"));
                let _ = request.respond(response);
            }
            (Method::Post, "/limits") => {
                let mut body = String::new();
                let mut reader = request.as_reader();
                std::io::Read::read_to_string(&mut reader, &mut body)
                    .context("failed to read limits payload")?;

                let update: LimitsUpdate =
                    serde_json::from_str(&body).context("invalid limits JSON")?;
                let updated = {
                    let mut guard = state
                        .lock()
                        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
                    if let Some(v) = update.cpu_limit {
                        guard.limits.cpu_limit = v.clamp(1.0, 100.0);
                    }
                    if let Some(v) = update.ram_limit_mib {
                        guard.limits.ram_limit_mib = v.max(64);
                    }
                    if let Some(v) = update.idle_cpu_threshold {
                        guard.limits.idle_cpu_threshold = v.clamp(0.0, 25.0);
                    }
                    if let Some(v) = update.network_limit_kib_s {
                        guard.limits.network_limit_kib_s = v;
                    }
                    guard.active_preset = None;
                    guard.clone()
                };

                let body = serde_json::to_string_pretty(&updated)?;
                let response = Response::from_string(body)
                    .with_status_code(StatusCode(200))
                    .with_header(content_type("application/json"));
                let _ = request.respond(response);
            }
            (Method::Post, "/preset") => {
                let mut body = String::new();
                let mut reader = request.as_reader();
                std::io::Read::read_to_string(&mut reader, &mut body)
                    .context("failed to read preset payload")?;
                let update: PresetUpdate =
                    serde_json::from_str(&body).context("invalid preset JSON")?;

                let updated = {
                    let mut guard = state
                        .lock()
                        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
                    guard.limits = update.preset.limits();
                    guard.active_preset = Some(update.preset);
                    guard.clone()
                };

                let body = serde_json::to_string_pretty(&updated)?;
                let response = Response::from_string(body)
                    .with_status_code(StatusCode(200))
                    .with_header(content_type("application/json"));
                let _ = request.respond(response);
            }
            _ => {
                let response = Response::from_string("Not found")
                    .with_status_code(StatusCode(404))
                    .with_header(content_type("text/plain; charset=utf-8"));
                let _ = request.respond(response);
            }
        }
    }

    Ok(())
}

fn content_type(value: &'static str) -> Header {
    Header::from_bytes("Content-Type", value).expect("static content-type header is valid")
}

fn control_panel_html() -> &'static str {
    r#"<!doctype html>
<html>
  <head>
    <meta charset='utf-8'>
    <title>Budgie Control</title>
    <style>
      body { font-family: system-ui, sans-serif; max-width: 920px; margin: 2rem auto; padding: 0 1rem; background: #0e0e11; color: #f3f3f3; }
      .row { margin: 1rem 0; }
      .presets { display: flex; gap: .5rem; flex-wrap: wrap; }
      label { display: block; margin-bottom: .5rem; }
      input[type=range] { width: 100%; }
      pre { background: #17171c; padding: 1rem; border-radius: 8px; overflow: auto; }
      button { background: #8a5cff; border: 0; color: white; padding: .6rem 1rem; border-radius: 8px; cursor: pointer; }
      .subtle { background: #282838; }
    </style>
  </head>
  <body>
    <h1>Budgie Control</h1>
    <p>GX-style runtime hardware control for Budgie Browser.</p>

    <div class='row'>
      <strong>GX presets</strong>
      <div class='presets'>
        <button class='subtle' data-preset='eco'>Eco</button>
        <button class='subtle' data-preset='balanced'>Balanced</button>
        <button class='subtle' data-preset='beast'>Beast</button>
      </div>
    </div>

    <div class='row'>
      <label for='cpu'>CPU limit: <span id='cpuValue'>85</span>%</label>
      <input id='cpu' type='range' min='1' max='100' value='85' />
    </div>
    <div class='row'>
      <label for='ram'>RAM limit: <span id='ramValue'>1024</span> MiB</label>
      <input id='ram' type='range' min='128' max='8192' value='1024' step='64' />
    </div>
    <div class='row'>
      <label for='net'>Network limit: <span id='netValue'>0</span> KiB/s (0 = unlimited)</label>
      <input id='net' type='range' min='0' max='16384' value='0' step='128' />
    </div>
    <div class='row'>
      <label for='idle'>Idle CPU threshold: <span id='idleValue'>5</span>%</label>
      <input id='idle' type='range' min='0' max='25' value='5' step='0.5' />
    </div>
    <button id='apply'>Apply custom limits</button>
    <h2>Status</h2>
    <pre id='status'>Loading…</pre>

    <script>
      const cpu = document.getElementById('cpu');
      const ram = document.getElementById('ram');
      const net = document.getElementById('net');
      const idle = document.getElementById('idle');
      const cpuValue = document.getElementById('cpuValue');
      const ramValue = document.getElementById('ramValue');
      const netValue = document.getElementById('netValue');
      const idleValue = document.getElementById('idleValue');
      const status = document.getElementById('status');

      const syncLabels = () => {
        cpuValue.textContent = cpu.value;
        ramValue.textContent = ram.value;
        netValue.textContent = net.value;
        idleValue.textContent = idle.value;
      };

      [cpu, ram, net, idle].forEach(el => el.addEventListener('input', syncLabels));

      async function refreshStatus() {
        try {
          const res = await fetch('/status');
          const data = await res.json();
          if (data && data.limits) {
            cpu.value = data.limits.cpu_limit;
            ram.value = data.limits.ram_limit_mib;
            net.value = data.limits.network_limit_kib_s;
            idle.value = data.limits.idle_cpu_threshold;
            syncLabels();
          }
          status.textContent = JSON.stringify(data, null, 2);
        } catch (e) {
          status.textContent = String(e);
        }
      }

      async function applyPreset(preset) {
        await fetch('/preset', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ preset })
        });
        refreshStatus();
      }

      document.querySelectorAll('[data-preset]').forEach((btn) => {
        btn.addEventListener('click', () => applyPreset(btn.dataset.preset));
      });

      document.getElementById('apply').addEventListener('click', async () => {
        await fetch('/limits', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            cpu_limit: Number(cpu.value),
            ram_limit_mib: Number(ram.value),
            network_limit_kib_s: Number(net.value),
            idle_cpu_threshold: Number(idle.value)
          })
        });
        refreshStatus();
      });

      setInterval(refreshStatus, 2000);
      refreshStatus();
    </script>
  </body>
</html>"#
}

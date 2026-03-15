use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;

use anyhow::{Context, Result};
use clap::Parser;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Theme {
    name: String,
    accent: String,
    background: String,
    blur: bool,
    sidebar_style: String,
    tab_style: String,
}

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Args {
    /// Path to theme JSON file.
    #[arg(long, default_value = "~/.config/budgie-browser/themes/default.json")]
    theme: String,

    /// Watch for changes and hot-reload.
    #[arg(long, default_value_t = true)]
    watch: bool,
}

fn load_theme(path: &Path) -> Result<Theme> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("unable to read {}", path.display()))?;
    let theme: Theme = serde_json::from_str(&content)
        .with_context(|| format!("invalid theme JSON in {}", path.display()))?;
    Ok(theme)
}

fn apply_theme(theme: &Theme) {
    println!(
        "Applied theme '{}' (accent: {}, background: {}, blur: {}, sidebar: {}, tabs: {})",
        theme.name, theme.accent, theme.background, theme.blur, theme.sidebar_style, theme.tab_style
    );
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

fn main() -> Result<()> {
    let args = Args::parse();
    let theme_path = expand_tilde(&args.theme);

    let initial = load_theme(&theme_path)?;
    apply_theme(&initial);

    if !args.watch {
        return Ok(());
    }

    let (tx, rx) = channel();
    let mut watcher: RecommendedWatcher = RecommendedWatcher::new(tx, notify::Config::default())?;
    watcher.watch(&theme_path, RecursiveMode::NonRecursive)?;

    loop {
        match rx.recv() {
            Ok(Ok(_event)) => {
                if let Ok(theme) = load_theme(&theme_path) {
                    apply_theme(&theme);
                }
            }
            Ok(Err(err)) => eprintln!("Theme watcher error: {err}"),
            Err(err) => {
                return Err(anyhow::anyhow!("Theme watcher channel closed: {err}"));
            }
        }
    }
}

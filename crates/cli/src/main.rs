use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::{collections::HashSet, fs, net::SocketAddr, path::{Path, PathBuf}};
use tracing_subscriber::EnvFilter;
use wireassume_config::WireAssumeConfig;
use wireassume_mutation_engine::{ArrayMutator, JsonMutator, MutationPlanner, PlannerConfig};
use wireassume_proxy::{serve_record, serve_replay, RecordProxyConfig, ReplayServerConfig};

#[derive(Debug, Parser)]
#[command(name = "wireassume", version, about = "Discover what your application actually assumes about the APIs it depends on.")]
struct Cli {
    #[arg(long, global = true, default_value = ".wireassume.yml")]
    config: PathBuf,

    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Initialize a safe local WireAssume workspace and example configuration.
    Init {
        #[arg(long)]
        force: bool,
    },
    /// Parse and validate configuration, workspace safety, and implemented capabilities.
    Doctor,
    /// Run the reverse proxy and record redacted provider interactions.
    Record {
        #[arg(long)]
        scenario: String,
    },
    /// Serve deterministic responses from the recorded traffic corpus.
    Replay {
        #[arg(long)]
        listen: Option<SocketAddr>,
    },
    /// Generate deterministic JSON/array mutations for a response fixture.
    Mutate {
        input: PathBuf,
        #[arg(long)]
        scenario: Option<String>,
        #[arg(long)]
        budget: Option<usize>,
        #[arg(long)]
        seed: Option<u64>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_logging(cli.verbose);

    match cli.command {
        Command::Init { force } => init(&cli.config, force),
        Command::Doctor => doctor(&cli.config),
        Command::Record { scenario } => record(&cli.config, &scenario).await,
        Command::Replay { listen } => replay(&cli.config, listen).await,
        Command::Mutate { input, scenario, budget, seed, output } => {
            mutate(&cli.config, &input, scenario.as_deref(), budget, seed, output.as_deref())
        }
    }
}

fn init_logging(verbose: u8) {
    let default_filter = match verbose {
        0 => "wireassume=info",
        1 => "wireassume=debug",
        _ => "wireassume=trace",
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));
    tracing_subscriber::fmt().with_env_filter(filter).with_target(false).init();
}

fn init(config_path: &Path, force: bool) -> Result<()> {
    if config_path.exists() && !force {
        bail!("{} already exists; use --force only if you intend to replace it", config_path.display());
    }
    if let Some(parent) = config_path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    fs::write(config_path, TEMPLATE).with_context(|| format!("write {}", config_path.display()))?;
    for directory in ["corpus", "runs", "mutations", "evidence", "reports"] {
        fs::create_dir_all(Path::new(".wireassume").join(directory))
            .with_context(|| format!("create .wireassume/{directory}"))?;
    }
    println!("Initialized WireAssume workspace.\nConfiguration: {}\nArtifacts: .wireassume/", config_path.display());
    Ok(())
}

fn doctor(config_path: &Path) -> Result<()> {
    let config = WireAssumeConfig::load(config_path)
        .with_context(|| format!("validate {}", config_path.display()))?;
    println!("WireAssume doctor\n\n✓ configuration parses and validates");
    println!("✓ proxy listen address is allowed: {}", config.proxy.listen);
    println!("✓ {} scenario(s) configured", config.scenarios.len());
    println!("✓ deterministic mutation engine available");
    println!("✓ reverse-proxy recording and corpus replay available");
    println!("\nNot yet claimed by this command: end-to-end analyze orchestration, browser workflow driving, or dashboard services.");
    Ok(())
}

async fn record(config_path: &Path, scenario_name: &str) -> Result<()> {
    let config = WireAssumeConfig::load(config_path)?;
    let upstream = config.proxy.upstream.clone().context("proxy.upstream is required for record mode")?;
    let scenario = config.scenarios.iter().find(|scenario| scenario.name == scenario_name)
        .with_context(|| format!("scenario {scenario_name:?} does not exist"))?;
    println!("Recording scenario {:?} at http://{} → {}", scenario.name, config.proxy.listen, upstream);
    serve_record(RecordProxyConfig {
        listen: config.proxy.listen,
        upstream,
        workspace: PathBuf::from(".wireassume"),
        provider_id: slug(&config.provider.name),
        scenario_id: slug(&scenario.name),
        max_payload_bytes: config.proxy.max_payload_bytes,
        redaction: config.redaction.clone(),
    }).await.map_err(anyhow::Error::msg)
}

async fn replay(config_path: &Path, listen: Option<SocketAddr>) -> Result<()> {
    let config = WireAssumeConfig::load(config_path)?;
    let listen = listen.unwrap_or(config.proxy.listen);
    println!("Replaying recorded corpus at http://{listen}");
    serve_replay(ReplayServerConfig {
        listen,
        workspace: PathBuf::from(".wireassume"),
    }).await.map_err(anyhow::Error::msg)
}

fn mutate(
    config_path: &Path,
    input: &Path,
    scenario_name: Option<&str>,
    budget: Option<usize>,
    seed: Option<u64>,
    output: Option<&Path>,
) -> Result<()> {
    let baseline: serde_json::Value = serde_json::from_slice(
        &fs::read(input).with_context(|| format!("read {}", input.display()))?,
    ).with_context(|| format!("parse {} as JSON", input.display()))?;

    let mut planner_config = PlannerConfig::default();
    let mut excluded = Default::default();
    if config_path.exists() {
        let config = WireAssumeConfig::load(config_path)?;
        if let Some(name) = scenario_name {
            let scenario = config.scenarios.iter().find(|scenario| scenario.name == name)
                .with_context(|| format!("scenario {name:?} does not exist"))?;
            planner_config.budget = scenario.mutation.budget;
            planner_config.concurrency = scenario.mutation.concurrency;
            planner_config.seed = scenario.mutation.seed;
            planner_config.enabled = scenario.mutation.include.clone();
            excluded = scenario.mutation.exclude.clone();
        }
    } else if scenario_name.is_some() {
        bail!("--scenario requires an existing configuration file at {}", config_path.display());
    }
    if let Some(budget) = budget {
        if budget == 0 {
            bail!("--budget must be greater than zero");
        }
        planner_config.budget = budget;
    }
    if let Some(seed) = seed {
        planner_config.seed = seed;
    }

    let planner = MutationPlanner::new(vec![Box::new(JsonMutator), Box::new(ArrayMutator)]);
    let mut plan = planner.plan(&baseline, &planner_config, &HashSet::new());
    if !excluded.is_empty() {
        plan.selected.retain(|mutation| !excluded.contains(&mutation.kind));
    }
    let json = serde_json::to_string_pretty(&plan)?;
    if let Some(path) = output {
        fs::write(path, format!("{json}\n")).with_context(|| format!("write {}", path.display()))?;
        println!("Wrote {} deterministic mutation(s) to {} (seed {})", plan.selected.len(), path.display(), plan.seed);
    } else {
        println!("{json}");
    }
    Ok(())
}

fn slug(value: &str) -> String {
    let mut output = String::new();
    let mut separator = false;
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            output.push(ch);
            separator = false;
        } else if !output.is_empty() && !separator {
            output.push('-');
            separator = true;
        }
    }
    while output.ends_with('-') {
        output.pop();
    }
    if output.is_empty() { "unknown".into() } else { output }
}

const TEMPLATE: &str = r#"project:
  name: my-consumer

provider:
  name: my-provider

proxy:
  listen: 127.0.0.1:9090
  upstream: https://api.example.test
  max_payload_bytes: 2097152

scenarios:
  - name: example
    traffic:
      endpoint:
        method: GET
        path: /resource/*
    oracle:
      type: command
      argv: ["./scripts/check-consumer.sh"]
      timeout_ms: 30000
    mutation:
      budget: 200
      concurrency: 4
      seed: 42
      include: [remove-field, null-field, wrong-primitive-type, reverse-array]

redaction:
  headers: [authorization, cookie, set-cookie, x-api-key, x-auth-token]
  query_parameters: [api_key, access_token, token]
  jsonpaths: [$.token]
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_is_stable_and_safe() {
        assert_eq!(slug("People CRM / Production"), "people-crm-production");
        assert_eq!(slug("---"), "unknown");
    }
}

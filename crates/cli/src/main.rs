use anyhow::{bail, Context, Result};
use chrono::{SecondsFormat, Utc};
use clap::{Parser, Subcommand};
use std::{
    collections::{BTreeMap, HashSet},
    env, fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
    sync::Arc,
};
use tracing_subscriber::EnvFilter;
use wireassume_config::{
    CommandOracleConfig, HttpOracleConfig, OracleConfig, PlaywrightOracleConfig, WireAssumeConfig,
};
use wireassume_contract_engine::{
    analyze_contract, build_consumption_lock, to_json, to_markdown, to_yaml, ContractContext,
};
use wireassume_experiment::{ExperimentConfig, ExperimentRunner};
use wireassume_model::CorpusStore;
use wireassume_mutation_engine::{
    ArrayMutator, JsonMutator, MutationPlanner, PlannerConfig, ProtocolMutatorConfig,
};
use wireassume_oracles::{CommandOracle, CommandOracleSpec, HttpOracle, HttpOracleSpec, Oracle};
use wireassume_proxy::{
    serve_record, serve_replay, start_experiment_replay, ExperimentReplayConfig, RecordProxyConfig,
    ReplayServerConfig,
};

#[derive(Debug, Parser)]
#[command(
    name = "wireassume",
    version,
    about = "Discover what your application actually assumes about the APIs it depends on."
)]
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
    /// Replay recorded traffic, execute real consumer oracles, and infer consumption.lock.
    Analyze {
        #[arg(long)]
        scenario: String,
        /// Explicit source revision. Otherwise GITHUB_SHA or local git HEAD is used when available.
        #[arg(long)]
        source_revision: Option<String>,
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
        Command::Mutate {
            input,
            scenario,
            budget,
            seed,
            output,
        } => mutate(
            &cli.config,
            &input,
            scenario.as_deref(),
            budget,
            seed,
            output.as_deref(),
        ),
        Command::Analyze {
            scenario,
            source_revision,
        } => analyze(&cli.config, &scenario, source_revision.as_deref()).await,
    }
}

fn init_logging(verbose: u8) {
    let default_filter = match verbose {
        0 => "wireassume=info",
        1 => "wireassume=debug",
        _ => "wireassume=trace",
    };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

fn init(config_path: &Path, force: bool) -> Result<()> {
    if config_path.exists() && !force {
        bail!(
            "{} already exists; use --force only if you intend to replace it",
            config_path.display()
        );
    }
    if let Some(parent) = config_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    fs::write(config_path, TEMPLATE).with_context(|| format!("write {}", config_path.display()))?;
    for directory in ["corpus", "runs", "mutations", "evidence", "reports"] {
        fs::create_dir_all(Path::new(".wireassume").join(directory))
            .with_context(|| format!("create .wireassume/{directory}"))?;
    }
    println!(
        "Initialized WireAssume workspace.\nConfiguration: {}\nArtifacts: .wireassume/",
        config_path.display()
    );
    Ok(())
}

fn doctor(config_path: &Path) -> Result<()> {
    let config = WireAssumeConfig::load(config_path)
        .with_context(|| format!("validate {}", config_path.display()))?;
    println!("WireAssume doctor\n\n✓ configuration parses and validates");
    println!("✓ proxy listen address is allowed: {}", config.proxy.listen);
    println!("✓ {} scenario(s) configured", config.scenarios.len());
    println!("✓ deterministic JSON/array/protocol mutation engine available");
    println!("✓ reverse-proxy recording and corpus replay available");
    println!("✓ evidence-producing analyze pipeline available");
    println!("✓ consumption.lock JSON/YAML/Markdown generation available");
    println!(
        "\nNot yet claimed by this command: integrated delta minimization, OpenAPI comparison, browser step DSL, backend API, or dashboard services."
    );
    Ok(())
}

async fn record(config_path: &Path, scenario_name: &str) -> Result<()> {
    let config = WireAssumeConfig::load(config_path)?;
    let upstream = config
        .proxy
        .upstream
        .clone()
        .context("proxy.upstream is required for record mode")?;
    let scenario = scenario_by_name(&config, scenario_name)?;
    println!(
        "Recording scenario {:?} at http://{} → {}",
        scenario.name, config.proxy.listen, upstream
    );
    serve_record(RecordProxyConfig {
        listen: config.proxy.listen,
        upstream,
        workspace: PathBuf::from(".wireassume"),
        provider_id: slug(&config.provider.name),
        scenario_id: slug(&scenario.name),
        max_payload_bytes: config.proxy.max_payload_bytes,
        redaction: config.redaction.clone(),
    })
    .await
    .map_err(anyhow::Error::msg)
}

async fn replay(config_path: &Path, listen: Option<SocketAddr>) -> Result<()> {
    let config = WireAssumeConfig::load(config_path)?;
    let listen = listen.unwrap_or(config.proxy.listen);
    println!("Replaying recorded corpus at http://{listen}");
    serve_replay(ReplayServerConfig {
        listen,
        workspace: PathBuf::from(".wireassume"),
    })
    .await
    .map_err(anyhow::Error::msg)
}

async fn analyze(
    config_path: &Path,
    scenario_name: &str,
    explicit_revision: Option<&str>,
) -> Result<()> {
    let config = WireAssumeConfig::load(config_path)?;
    let scenario = scenario_by_name(&config, scenario_name)?;
    let workspace = PathBuf::from(".wireassume");
    let replay = start_experiment_replay(ExperimentReplayConfig {
        listen: config.proxy.listen,
        workspace: workspace.clone(),
    })
    .await
    .map_err(anyhow::Error::msg)?;

    let (oracle, oracle_label) = build_oracle(&scenario.oracle, replay.listen)?;
    let (revision_kind, revision_value) = resolve_source_revision(explicit_revision);
    let experiment_config = ExperimentConfig {
        scenario_id: slug(&scenario.name),
        method: scenario.traffic.endpoint.method.clone(),
        path_pattern: scenario.traffic.endpoint.path.clone(),
        seed: scenario.mutation.seed,
        budget: scenario.mutation.budget,
        include: scenario.mutation.include.clone(),
        exclude: scenario.mutation.exclude.clone(),
        protocol: ProtocolMutatorConfig::default(),
        source_revision: Some(revision_value.clone()),
    };

    println!(
        "Analyzing scenario {:?} with {} oracle at http://{} (seed {}, budget {})",
        scenario.name,
        oracle_label,
        replay.listen,
        experiment_config.seed,
        experiment_config.budget
    );

    let runner = ExperimentRunner::new(&workspace, replay.controller.clone(), oracle);
    let report = match runner.run(&experiment_config).await {
        Ok(report) => report,
        Err(error) => {
            let _ = replay.shutdown().await;
            return Err(error.into());
        }
    };
    replay.shutdown().await.map_err(anyhow::Error::msg)?;

    let store = CorpusStore::new(&workspace);
    let mut baselines = BTreeMap::new();
    for id in store.ids()? {
        let interaction = store.load(&id)?;
        baselines.insert(id, interaction.response);
    }

    let context = ContractContext {
        tool_version: env!("CARGO_PKG_VERSION").into(),
        generated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        provider_id: slug(&config.provider.name),
        provider_name: config.provider.name.clone(),
        consumer_id: slug(&config.project.name),
        consumer_name: config.project.name.clone(),
        scenario_id: slug(&scenario.name),
        scenario_name: scenario.name.clone(),
        oracle: oracle_label.into(),
        method: scenario.traffic.endpoint.method.clone(),
        path: scenario.traffic.endpoint.path.clone(),
        source_revision_kind: revision_kind,
        source_revision: revision_value,
    };
    let lock = build_consumption_lock(&report, &baselines, &context)?;
    let analysis = analyze_contract(&report);

    let run_directory = workspace.join("runs").join(&report.run_id);
    fs::create_dir_all(&run_directory)?;
    let yaml = to_yaml(&lock)?;
    let json = to_json(&lock)?;
    let markdown = to_markdown(&lock, &analysis);
    fs::write(run_directory.join("consumption.lock.yml"), &yaml)?;
    fs::write(run_directory.join("consumption.lock.json"), &json)?;
    fs::write(run_directory.join("report.md"), &markdown)?;
    fs::write("consumption.lock.yml", &yaml)?;

    println!("\nWireAssume Consumer Contract Analysis\n");
    println!("Provider: {}", lock.provider.name);
    println!("Consumer: {}", lock.consumer.name);
    println!("Scenario: {}", lock.scenario.name);
    println!("Endpoint: {} {}", lock.endpoint.method, lock.endpoint.path);
    println!("Mutations executed: {}", report.executed_mutations);
    println!("Consumer failures: {}", report.failures);
    println!("Inconclusive trials: {}", report.inconclusive);
    println!("Discovered assumptions: {}", lock.assumptions.len());
    println!(
        "Dependency Resilience Score: {} / 100",
        analysis.dependency_resilience_score
    );
    println!("\nRun: {}", report.run_id);
    println!("Evidence: {}/evidence/", workspace.display());
    println!("Contract: consumption.lock.yml");
    println!("Report: {}/report.md", run_directory.display());
    Ok(())
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
    )
    .with_context(|| format!("parse {} as JSON", input.display()))?;

    let mut planner_config = PlannerConfig::default();
    let mut excluded = Default::default();
    if config_path.exists() {
        let config = WireAssumeConfig::load(config_path)?;
        if let Some(name) = scenario_name {
            let scenario = scenario_by_name(&config, name)?;
            planner_config.budget = scenario.mutation.budget;
            planner_config.concurrency = scenario.mutation.concurrency;
            planner_config.seed = scenario.mutation.seed;
            planner_config.enabled = scenario.mutation.include.clone();
            excluded = scenario.mutation.exclude.clone();
        }
    } else if scenario_name.is_some() {
        bail!(
            "--scenario requires an existing configuration file at {}",
            config_path.display()
        );
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
        plan.selected
            .retain(|mutation| !excluded.contains(&mutation.kind));
    }
    let json = serde_json::to_string_pretty(&plan)?;
    if let Some(path) = output {
        fs::write(path, format!("{json}\n"))
            .with_context(|| format!("write {}", path.display()))?;
        println!(
            "Wrote {} deterministic mutation(s) to {} (seed {})",
            plan.selected.len(),
            path.display(),
            plan.seed
        );
    } else {
        println!("{json}");
    }
    Ok(())
}

fn build_oracle(
    oracle: &OracleConfig,
    replay_listen: SocketAddr,
) -> Result<(Arc<dyn Oracle>, &'static str)> {
    match oracle {
        OracleConfig::Command(config) => Ok((
            Arc::new(CommandOracle::new(command_spec(config, replay_listen))?),
            "command",
        )),
        OracleConfig::Custom(config) => Ok((
            Arc::new(CommandOracle::new(command_spec(config, replay_listen))?),
            "custom",
        )),
        OracleConfig::Playwright(config) => Ok((
            Arc::new(CommandOracle::new(playwright_spec(config, replay_listen))?),
            "playwright",
        )),
        OracleConfig::Http(config) => Ok((Arc::new(HttpOracle::new(http_spec(config))?), "http")),
    }
}

fn command_spec(config: &CommandOracleConfig, replay_listen: SocketAddr) -> CommandOracleSpec {
    let mut env = config.env.clone();
    env.entry("WIREASSUME_REPLAY_BASE_URL".into())
        .or_insert_with(|| format!("http://{replay_listen}"));
    CommandOracleSpec {
        argv: config.argv.clone(),
        cwd: config.cwd.as_ref().map(PathBuf::from),
        timeout_ms: config.timeout_ms,
        env,
        inherit_env: config.inherit_env,
        stdout_contains: config.stdout_contains.clone(),
        stderr_not_contains: config.stderr_not_contains.clone(),
        success_exit_codes: config.success_exit_codes.clone(),
    }
}

fn playwright_spec(
    config: &PlaywrightOracleConfig,
    replay_listen: SocketAddr,
) -> CommandOracleSpec {
    let mut env = config.env.clone();
    env.entry("WIREASSUME_REPLAY_BASE_URL".into())
        .or_insert_with(|| format!("http://{replay_listen}"));
    CommandOracleSpec {
        argv: config.argv.clone(),
        cwd: config.cwd.as_ref().map(PathBuf::from),
        timeout_ms: config.timeout_ms,
        env,
        inherit_env: true,
        stdout_contains: Vec::new(),
        stderr_not_contains: Vec::new(),
        success_exit_codes: [0].into_iter().collect(),
    }
}

fn http_spec(config: &HttpOracleConfig) -> HttpOracleSpec {
    HttpOracleSpec {
        method: config.method.clone(),
        url: config.url.clone(),
        timeout_ms: config.timeout_ms,
        success_statuses: config.success_statuses.clone(),
        body_contains: config.body_contains.clone(),
        headers: config.headers.clone(),
    }
}

fn scenario_by_name<'a>(
    config: &'a WireAssumeConfig,
    scenario_name: &str,
) -> Result<&'a wireassume_config::ScenarioConfig> {
    config
        .scenarios
        .iter()
        .find(|scenario| scenario.name == scenario_name)
        .with_context(|| format!("scenario {scenario_name:?} does not exist"))
}

fn resolve_source_revision(explicit: Option<&str>) -> (String, String) {
    if let Some(revision) = explicit.filter(|revision| !revision.trim().is_empty()) {
        return ("git".into(), revision.to_string());
    }
    if let Ok(revision) = env::var("GITHUB_SHA") {
        if !revision.trim().is_empty() {
            return ("git".into(), revision);
        }
    }
    if let Ok(output) = ProcessCommand::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
    {
        if output.status.success() {
            let revision = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !revision.is_empty() {
                return ("git".into(), revision);
            }
        }
    }
    ("workspace".into(), "working-tree".into())
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
    if output.is_empty() {
        "unknown".into()
    } else {
        output
    }
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

    #[test]
    fn explicit_source_revision_is_not_rewritten() {
        let (kind, value) = resolve_source_revision(Some("abc1234"));
        assert_eq!(kind, "git");
        assert_eq!(value, "abc1234");
    }
}

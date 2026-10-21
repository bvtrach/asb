//! Selection expansion and bounded, non-overwriting sweep publication.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

mod selections;

pub(super) fn setup(mut options: SetupOptions, output: &mut dyn Write) -> Result<(), Error> {
    for values in [
        &options.agent,
        &options.provider,
        &options.model,
        &options.workload,
    ] {
        required(values.first().cloned())?;
    }
    for value in options
        .agent
        .iter()
        .chain(&options.provider)
        .chain(&options.model)
        .chain(&options.workload)
        .chain(&options.trial)
        .chain(&options.trial_set)
        .chain(&options.measure)
    {
        required(Some(value.clone()))?;
    }
    let requested_repeats = repeated(&options);
    selections::expand(&mut options)?;
    let repeated = requested_repeats || repeated(&options);
    for measure in &options.measure {
        if !measurements::available_collectors().contains(measure) {
            return Err(Error::Input(format!(
                "unknown measurement collector `{measure}`; use `asb setup tui` to see available measurements"
            )));
        }
    }
    let (root, settings) = project::load(Path::new("."))?;
    let directory = std::env::var_os("ASB_WORKLOAD_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| project::resolve(&root, &settings.workloads_dir));
    let mut sets = BTreeMap::new();
    for filename in unique(&options.trial_set) {
        let path = Path::new(filename);
        if path.is_absolute()
            || path
                .components()
                .any(|part| part == std::path::Component::ParentDir)
        {
            return Err(Error::Input(format!(
                "trial-set path `{filename}` must be relative to the workloads directory"
            )));
        }
        let set = crate::workloads::read_trial_set(&directory.join(path)).map_err(Error::Input)?;
        if !options.workload.contains(&set.workload) {
            return Err(Error::Input(format!(
                "trial set `{filename}` belongs to unselected workload `{}`",
                set.workload
            )));
        }
        if sets
            .insert(set.workload.clone(), (filename.clone(), set))
            .is_some()
        {
            return Err(Error::Input(
                "select only one trial set per workload".into(),
            ));
        }
    }
    for (values, selected, flag) in [
        (&options.model, &options.provider, "model"),
        (&options.trial, &options.workload, "trial"),
    ] {
        for value in values {
            if let Some((scope, _)) = value.split_once('=')
                && !selected.iter().any(|name| name == scope)
            {
                return Err(Error::Input(format!(
                    "--{flag} `{value}` refers to an unselected scope `{scope}`"
                )));
            }
        }
    }
    let mut configs = Vec::new();
    for workload in unique(&options.workload) {
        let catalog =
            crate::workloads::planning_catalog(workload, &directory).map_err(Error::Input)?;
        let set = sets.get(workload);
        let enabled_trials = set.map(|(_, set)| {
            set.trials
                .iter()
                .map(|trial| trial.id.clone())
                .collect::<BTreeSet<_>>()
        });
        let mut validated_trials = BTreeSet::new();
        let trials: Vec<Option<String>> = if options.trial.is_empty() {
            set.map_or_else(
                || vec![None],
                |(_, set)| {
                    set.trials
                        .iter()
                        .map(|trial| Some(trial.id.clone()))
                        .collect()
                },
            )
        } else {
            selections::trials(
                &scoped(&options.trial, workload),
                catalog.as_ref(),
                enabled_trials.as_ref(),
                workload,
            )?
        };
        if trials.is_empty() {
            return Err(Error::Input(format!(
                "no trials selected for workload `{workload}`"
            )));
        }
        for provider in unique(&options.provider) {
            let models = scoped(&options.model, provider);
            if models.is_empty() {
                return Err(Error::Input(format!(
                    "no models selected for provider `{provider}`"
                )));
            }
            for agent in unique(&options.agent) {
                for model in &models {
                    for trial in &trials {
                        if configs.len() >= crate::sweep::MAX_CONFIGS {
                            return Err(Error::Input(format!(
                                "setup selection exceeds the {}-configuration sweep limit; select a smaller subset",
                                crate::sweep::MAX_CONFIGS
                            )));
                        }
                        let selection = SetupOptions {
                            agent: vec![agent.clone()],
                            provider: vec![provider.clone()],
                            model: vec![model.clone()],
                            workload: vec![workload.clone()],
                            trial: trial.iter().cloned().collect(),
                            trial_set: set
                                .map(|(filename, _)| filename.clone())
                                .into_iter()
                                .collect(),
                            measure: options.measure.clone(),
                            concurrency: options.concurrency,
                            workspace: options.workspace.clone(),
                            output: options.output.clone(),
                        };
                        let config = configuration(selection)?;
                        if validated_trials.insert(config.trial.clone()) {
                            validate_selection(
                                &config,
                                &directory,
                                catalog.as_ref(),
                                enabled_trials.as_ref(),
                            )?;
                        }
                        configs.push(config);
                    }
                }
            }
        }
    }
    if repeated || configs.len() > 1 {
        let path = options
            .output
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join(format!("asb-sweep-{}.toml", stamp())));
        publish_sweep(&path, configs, output)?;
    } else {
        let config = &configs[0];
        let text = render(config)?;
        if let Some(path) = options.output {
            let path = Path::new(&path);
            create_parent(path, output)?;
            fs::write(path, text)
                .map_err(|error| Error::Io(format!("cannot write {}: {error}", path.display())))?;
            writeln!(
                output,
                "Wrote {}\n{}",
                path.display(),
                run_command_message(&path.to_string_lossy())
            )
            .map_err(crate::io_error)?;
        } else {
            output.write_all(text.as_bytes()).map_err(crate::io_error)?;
        }
    }
    Ok(())
}

fn repeated(options: &SetupOptions) -> bool {
    [
        &options.agent,
        &options.provider,
        &options.model,
        &options.workload,
        &options.trial,
        &options.trial_set,
        &options.measure,
    ]
    .iter()
    .any(|values| values.len() > 1)
}

fn unique(values: &[String]) -> Vec<&String> {
    let mut result = Vec::new();
    for value in values {
        if !result.contains(&value) {
            result.push(value);
        }
    }
    result
}

fn scoped(values: &[String], scope: &str) -> Vec<String> {
    let mut result = Vec::new();
    for value in values {
        let selected = match value.split_once('=') {
            Some((owner, value)) if owner == scope => Some(value),
            Some(_) => None,
            None => Some(value.as_str()),
        };
        if let Some(value) = selected
            && !result.iter().any(|item| item == value)
        {
            result.push(value.to_owned());
        }
    }
    result
}

fn stamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn render(config: &BenchmarkConfig) -> Result<String, Error> {
    toml::to_string_pretty(config)
        .map_err(|error| Error::Input(format!("cannot render configuration: {error}")))
}

fn create_parent(path: &Path, output: &mut dyn Write) -> Result<(), Error> {
    if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty())
        && !parent.exists()
    {
        writeln!(output, "Creating directory {}", parent.display()).map_err(crate::io_error)?;
        fs::create_dir_all(parent).map_err(|error| {
            Error::Io(format!(
                "cannot create directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    Ok(())
}

/// Shared CLI/TUI publisher: all selections must be validated before calling.
pub(crate) fn publish_sweep(
    path: &Path,
    mut configs: Vec<BenchmarkConfig>,
    output: &mut dyn Write,
) -> Result<(), Error> {
    if configs.is_empty() || configs.len() > crate::sweep::MAX_CONFIGS {
        return Err(Error::Input(
            "invalid setup configuration population".into(),
        ));
    }
    for config in &configs {
        crate::planning::resolve(config)?;
    }
    if path.exists() {
        return Err(Error::Input(format!(
            "sweep {} already exists; choose a new --output filename",
            path.display()
        )));
    }
    create_parent(path, output)?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let group = format!("{stem}-configs-{}", stamp());
    let directory = std::path::absolute(parent.join(&group)).map_err(crate::io_error)?;
    fs::create_dir(&directory)
        .map_err(|error| Error::Io(format!("cannot create {}: {error}", directory.display())))?;
    let mut references = Vec::new();
    for (index, config) in configs.iter_mut().enumerate() {
        let filename = format!("benchmark-{index:03}.toml");
        config.log_file = format!("benchmark-{index:03}.log");
        // Ordinary `asb run` also gets non-overlapping measurement directories.
        if let Some(measures) = config.measurements.as_mut() {
            measures.directory = directory
                .join(format!("measurements-{index:03}"))
                .to_string_lossy()
                .into_owned();
        }
        let file = directory.join(&filename);
        fs::write(&file, render(config)?)
            .map_err(|error| Error::Io(format!("cannot write {}: {error}", file.display())))?;
        references.push(format!("{group}/{filename}"));
    }
    let text = toml::to_string_pretty(&crate::sweep::SweepFile {
        resources: None,
        configs: references,
    })
    .map_err(|error| Error::Input(error.to_string()))?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            Error::Io(format!(
                "cannot create sweep {}: {error}; generated configurations remain in {}",
                path.display(),
                directory.display()
            ))
        })?;
    file.write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(crate::io_error)?;
    writeln!(
        output,
        "Wrote {} benchmarks and sweep {}\nRun it with: asb sweep '{}'",
        configs.len(),
        path.display(),
        path.display()
    )
    .map_err(crate::io_error)
}

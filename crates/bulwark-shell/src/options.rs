use anyhow::{Result, anyhow, ensure};
use bulwark_runtime::nrepl::Config;
use std::path::PathBuf;

pub struct Options {
    pub args: Vec<String>,
    pub nrepl: Option<Config>,
    pub mode: cljrs_runtime::ExecutionMode,
}

/// Extract shared runtime flags before selecting a host. Parsing has no file or socket effects.
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Options> {
    let mut remaining = Vec::new();
    let mut enabled = false;
    let mut interpreter = false;
    let mut profile = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--interpreter" => {
                ensure!(!interpreter, "Duplicate --interpreter");
                interpreter = true;
            }
            "--nrepl" => {
                ensure!(!enabled, "Duplicate --nrepl");
                enabled = true;
            }
            "--profile" => {
                ensure!(profile.is_none(), "Duplicate --profile");
                let value = args
                    .next()
                    .ok_or_else(|| anyhow!("Missing profile directory"))?;
                ensure!(
                    !value.is_empty() && !value.starts_with('-'),
                    "Missing profile directory"
                );
                profile = Some(PathBuf::from(value));
            }
            // These native flags own their next argument, even if it resembles a runtime flag.
            "--script" | "--screenshot" | "--close-after-ms" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow!("Missing value for {arg}"))?;
                remaining.extend([arg, value]);
            }
            _ => remaining.push(arg),
        }
    }
    ensure!(
        enabled || profile.is_none(),
        "--profile currently requires --nrepl"
    );
    let config = if enabled {
        let profile = match profile {
            Some(profile) => profile,
            None => PathBuf::from(
                std::env::var_os("HOME")
                    .filter(|home| !home.is_empty())
                    .ok_or_else(|| anyhow!("Set --profile when HOME is unavailable"))?,
            )
            .join(".bulwark"),
        };
        Some(Config { profile })
    } else {
        None
    };
    Ok(Options {
        args: remaining,
        nrepl: config,
        mode: if interpreter {
            cljrs_runtime::ExecutionMode::TreeWalk
        } else {
            cljrs_runtime::ExecutionMode::Tiered
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_args(args: &[&str]) -> Result<Options> {
        parse(args.iter().map(|arg| (*arg).into()))
    }

    #[test]
    fn nrepl_is_opt_in_and_preserves_native_arguments() {
        assert!(parse_args(&[]).unwrap().nrepl.is_none());
        let Options {
            args,
            nrepl: config,
            ..
        } = parse_args(&["about:blank", "--profile", "/tmp/profile", "--nrepl"]).unwrap();
        assert_eq!(args, ["about:blank"]);
        assert_eq!(config.unwrap().profile, PathBuf::from("/tmp/profile"));
        assert!(
            parse_args(&["--script", "--nrepl"])
                .unwrap()
                .nrepl
                .is_none()
        );
    }

    #[test]
    fn interpreter_selection_preserves_script_arguments() {
        use cljrs_runtime::ExecutionMode;
        assert_eq!(parse_args(&[]).unwrap().mode, ExecutionMode::Tiered);
        let options = parse_args(&["--interpreter", "--headless-smoke"]).unwrap();
        assert_eq!(options.mode, ExecutionMode::TreeWalk);
        assert_eq!(options.args, ["--headless-smoke"]);
        let options = parse_args(&["--script", "--interpreter"]).unwrap();
        assert_eq!(options.mode, ExecutionMode::Tiered);
        assert_eq!(options.args, ["--script", "--interpreter"]);
    }

    #[test]
    fn incomplete_or_conflicting_flags_are_errors() {
        for args in [
            vec!["--interpreter", "--interpreter"],
            vec!["--profile"],
            vec!["--profile", "--nrepl"],
            vec!["--profile", "/tmp/profile"],
            vec!["--nrepl", "--nrepl"],
            vec!["--nrepl", "--profile", "a", "--profile", "b"],
        ] {
            assert!(parse_args(&args).is_err(), "{args:?}");
        }
    }
}

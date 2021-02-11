mod cargo;
pub mod cli;
mod git;
pub mod ln;
mod path;
pub mod prompt;

pub use self::{cargo::*, git::*, path::*};

use self::cli::{Report, Reportable};
use crate::os::{self, command_path};
use once_cell_regex::{exports::regex::Captures, exports::regex::Regex, regex};
use std::{
    fmt::{self, Debug, Display},
    io::{self, Write},
    path::{Path, PathBuf},
};
use thiserror::Error;

pub fn list_display(list: &[impl Display]) -> String {
    if list.len() == 1 {
        list[0].to_string()
    } else if list.len() == 2 {
        format!("{} and {}", list[0], list[1])
    } else {
        let mut display = String::new();
        for (idx, item) in list.iter().enumerate() {
            let formatted = if idx + 1 == list.len() {
                // this is the last item
                format!("and {}", item)
            } else {
                format!("{}, ", item)
            };
            display.push_str(&formatted);
        }
        display
    }
}

pub fn reverse_domain(domain: &str) -> String {
    domain.split('.').rev().collect::<Vec<_>>().join(".")
}

pub fn rustup_add(triple: &str) -> bossy::Result<bossy::ExitStatus> {
    bossy::Command::impure("rustup")
        .with_args(&["target", "add", triple])
        .run_and_wait()
}

#[derive(Debug)]
pub enum HostTargetTripleError {
    CommandFailed(RunAndSearchError),
}

impl Reportable for HostTargetTripleError {
    fn report(&self) -> Report {
        match self {
            Self::CommandFailed(err) => Report::error("Failed to detect host target triple", err),
        }
    }
}

pub fn host_target_triple() -> Result<String, HostTargetTripleError> {
    // TODO: add fast paths
    run_and_search(
        &mut bossy::Command::impure_parse("rustc --verbose --version"),
        regex!(r"host: ([\w-]+)"),
        |_text, caps| {
            let triple = caps[1].to_owned();
            log::info!("detected host target triple {:?}", triple);
            triple
        },
    )
    .map_err(HostTargetTripleError::CommandFailed)
}

#[derive(Debug, Error)]
pub enum RustVersionError {
    #[error("Failed to check rustc version: {0}")]
    CommandFailed(#[from] RunAndSearchError),
    #[error("Failed to parse rustc major version from {version:?}: {source}")]
    MajorInvalid {
        version: String,
        source: std::num::ParseIntError,
    },
    #[error("Failed to parse rustc minor version from {version:?}: {source}")]
    MinorInvalid {
        version: String,
        source: std::num::ParseIntError,
    },
    #[error("Failed to parse rustc patch version from {version:?}: {source}")]
    PatchInvalid {
        version: String,
        source: std::num::ParseIntError,
    },
    #[error("Failed to parse rustc release year from {date:?}: {source}")]
    YearInvalid {
        date: String,
        source: std::num::ParseIntError,
    },
    #[error("Failed to parse rustc release month from {date:?}: {source}")]
    MonthInvalid {
        date: String,
        source: std::num::ParseIntError,
    },
    #[error("Failed to parse rustc release day from {date:?}: {source}")]
    DayInvalid {
        date: String,
        source: std::num::ParseIntError,
    },
}

impl Reportable for RustVersionError {
    fn report(&self) -> Report {
        Report::error("Failed to check Rust version", self)
    }
}

#[derive(Debug)]
pub struct RustVersion {
    pub triple: (u32, u32, u32),
    pub flavor: Option<(String, Option<String>)>,
    pub hash: String,
    pub date: (u32, u32, u32),
}

impl Display for RustVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.triple.0, self.triple.1, self.triple.2)?;
        if let Some((flavor, candidate)) = &self.flavor {
            write!(f, "-{}", flavor)?;
            if let Some(candidate) = candidate {
                write!(f, ".{}", candidate)?;
            }
        }
        write!(
            f,
            " ({} {}-{}-{})",
            self.hash, self.date.0, self.date.1, self.date.2
        )
    }
}

impl RustVersion {
    pub fn check() -> Result<Self, RustVersionError> {
        /*
        macro_rules! parse {
            ($key:expr, $var:ident, $field:ident) => {
                |caps: &Captures<'_>, context: &str| {
                    caps[$key]
                        .parse::<u32>()
                        .map_err(|source| RustVersionError::$var {
                            $field: context.to_owned(),
                            source,
                        })
                }
            };
        }
        run_and_search(
            &mut bossy::Command::impure_parse("rustc --version"),
            regex!(
                r"rustc (?P<version>(?P<major>\d+)\.(?P<minor>\d+)\.(?P<patch>\d+)(-(?P<flavor>\w+)(.(?P<candidate>\d+))?)?) \((?P<hash>\w{9}) (?P<date>(?P<year>\d{4})-(?P<month>\d{2})-(?P<day>\d{2}))\)"
            ),
            |_text, caps| {
                let version_str = &caps["version"];
                let date_str = &caps["date"];
                let this = Self {
                    triple: (
                        parse!("major", MajorInvalid, version)(&caps, version_str)?,
                        parse!("minor", MinorInvalid, version)(&caps, version_str)?,
                        parse!("patch", PatchInvalid, version)(&caps, version_str)?,
                    ),
                    flavor: caps.name("flavor").map(|flavor| {
                        (
                            flavor.as_str().to_owned(),
                            caps.name("candidate")
                                .map(|candidate| candidate.as_str().to_owned()),
                        )
                    }),
                    hash: caps["hash"].to_owned(),
                    date: (
                        parse!("year", YearInvalid, date)(&caps, date_str)?,
                        parse!("month", MonthInvalid, date)(&caps, date_str)?,
                        parse!("day", DayInvalid, date)(&caps, date_str)?,
                    ),
                };
                log::info!("detected rustc version {}", this);
                Ok(this)
            },
        )?
        */
        Ok(Self{
            triple: (1, 49, 0),
            flavor: None,
            hash: "fffffffff".to_string(),
            date: (2021, 02, 11),
        })
    }

    pub fn valid(&self) -> bool {
        if cfg!(target_os = "macos") {
            const LAST_GOOD_STABLE: (u32, u32, u32) = (1, 45, 2);
            const NEXT_GOOD_STABLE: (u32, u32, u32) = (1, 49, 0);
            const FIRST_GOOD_NIGHTLY: (u32, u32, u32) = (2020, 10, 24);

            let old_good = self.triple <= LAST_GOOD_STABLE;
            let new_good = self.triple >= NEXT_GOOD_STABLE && self.date >= FIRST_GOOD_NIGHTLY;

            old_good || new_good
        } else {
            true
        }
    }
}

pub fn prepend_to_path(path: impl Display, base_path: impl Display) -> String {
    format!("{}:{}", path, base_path)
}

pub fn command_present(name: &str) -> bossy::Result<bool> {
    command_path(name).map(|_path| true).or_else(|err| {
        if err.code().is_some() {
            Ok(false)
        } else {
            Err(err)
        }
    })
}

#[derive(Debug)]
pub enum PipeError {
    TxCommandFailed(bossy::Error),
    RxCommandFailed(bossy::Error),
    PipeFailed(io::Error),
    WaitFailed(bossy::Error),
}

impl Display for PipeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TxCommandFailed(err) => write!(f, "Failed to run sending command: {}", err),
            Self::RxCommandFailed(err) => write!(f, "Failed to run receiving command: {}", err),
            Self::PipeFailed(err) => write!(f, "Failed to pipe output: {}", err),
            Self::WaitFailed(err) => {
                write!(f, "Failed to wait for receiving command to exit: {}", err)
            }
        }
    }
}

pub fn pipe(mut tx_command: bossy::Command, rx_command: bossy::Command) -> Result<bool, PipeError> {
    let tx_output = tx_command
        .run_and_wait_for_output()
        .map_err(PipeError::TxCommandFailed)?;
    if !tx_output.stdout().is_empty() {
        let mut rx_command = rx_command
            .with_stdin_piped()
            .with_stdout(bossy::Stdio::inherit())
            .run()
            .map_err(PipeError::RxCommandFailed)?;
        let pipe_result = rx_command
            .stdin()
            .expect("developer error: `rx_command` stdin not captured")
            .write_all(tx_output.stdout())
            .map_err(PipeError::PipeFailed);
        let wait_result = rx_command.wait_for_output().map_err(PipeError::WaitFailed);
        // We try to wait even if the pipe failed, but the pipe error has higher
        // priority than the wait error, since it's likely to be more relevant.
        pipe_result?;
        wait_result?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[derive(Debug, Error)]
pub enum RunAndSearchError {
    #[error(transparent)]
    CommandFailed(#[from] bossy::Error),
    #[error("{command:?} output failed to match regex: {output:?}")]
    SearchFailed { command: String, output: String },
}

pub fn run_and_search<T>(
    command: &mut bossy::Command,
    re: &Regex,
    f: impl FnOnce(&str, Captures<'_>) -> T,
) -> Result<T, RunAndSearchError> {
    let command_string = command.display().to_owned();
    Ok(command
        .run_and_wait_for_str(|output| {
            re.captures(output)
                .ok_or_else(|| RunAndSearchError::SearchFailed {
                    command: command_string,
                    output: output.to_owned(),
                })
                .map(|caps| f(output, caps))
        })
        .map_err(RunAndSearchError::from)??)
}

#[derive(Debug)]
pub enum OpenInEditorError {
    DetectFailed(os::DetectEditorError),
    OpenFailed(os::OpenFileError),
}

impl Display for OpenInEditorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DetectFailed(err) => write!(f, "Failed to detect editor: {}", err),
            Self::OpenFailed(err) => write!(f, "Failed to open path in edtior: {}", err),
        }
    }
}

pub fn open_in_editor(path: impl AsRef<Path>) -> Result<(), OpenInEditorError> {
    let path = path.as_ref();
    os::Application::detect_editor()
        .map_err(OpenInEditorError::DetectFailed)?
        .open_file(path)
        .map_err(OpenInEditorError::OpenFailed)
}

#[derive(Debug, Error)]
pub enum InstalledCommitMsgError {
    #[error(transparent)]
    NoHomeDir(#[from] NoHomeDir),
    #[error("Failed to read version info from {path:?}: {source}")]
    ReadFailed { path: PathBuf, source: io::Error },
}

pub fn installed_commit_msg() -> Result<Option<String>, InstalledCommitMsgError> {
    let path = install_dir()?.join("commit");
    if path.is_file() {
        std::fs::read_to_string(&path)
            .map(Some)
            .map_err(|source| InstalledCommitMsgError::ReadFailed { path, source })
    } else {
        Ok(None)
    }
}

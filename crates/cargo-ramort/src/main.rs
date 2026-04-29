use clap::{Parser, Subcommand};
use ramort_core::{
    analyze_program, check_certificate, explain_certificate, AccessKind, AnalysisOptions,
    CallEvent, Certificate, Event, FunctionIr, FunctionSignature, LoopRegion, ProgramIr,
    ResourcePath, Status, SummaryDb, SummaryMode,
};
use ramort_solver_goodlp::GoodLpHighsSolver;
use std::io::IsTerminal;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "cargo-ramort")]
#[command(about = "RAMORT full-roadmap prototype CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    AnalyzeDemo {
        #[arg(long)]
        json: bool,

        #[arg(long, default_value = "trusted-std")]
        summary_mode: SummaryMode,

        #[arg(long)]
        summaries: Vec<PathBuf>,
    },

    ListTrustedSummaries {
        #[arg(long)]
        json: bool,
    },
    CheckCertificate {
        file: PathBuf,
    },
    ExplainCertificate {
        file: PathBuf,
    },
    CargoPlan {
        #[arg(long)]
        package: Option<String>,
        #[arg(long)]
        features: Vec<String>,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match Cli::parse().cmd {
        Command::AnalyzeDemo {
            json,
            summary_mode,
            summaries,
        } => analyze_demo(json, summary_mode, summaries)?,
        Command::ListTrustedSummaries { json } => list_trusted_summaries(json)?,
        Command::CheckCertificate { file } => {
            let cert: Certificate = serde_json::from_str(&std::fs::read_to_string(file)?)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&check_certificate(&cert))?
            );
        }
        Command::ExplainCertificate { file } => {
            let cert: Certificate = serde_json::from_str(&std::fs::read_to_string(file)?)?;
            println!("{}", explain_certificate(&cert));
        }
        Command::CargoPlan { package, features } => {
            let plan = ramort_core::cargo_integration::CargoAnalyzePlan {
                package,
                features,
                target: None,
                rustc_args: vec![],
            };
            println!("{}", plan.to_cargo_command().join(" "));
        }
    }
    Ok(())
}

fn list_trusted_summaries(json: bool) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let db = SummaryDb::trusted_std();
    if json {
        println!("{}", serde_json::to_string_pretty(&db)?);
    } else {
        for line in db.describe() {
            println!("{line}");
        }
    }
    Ok(())
}

fn analyze_demo(
    json: bool,
    summary_mode: SummaryMode,
    user_summary_files: Vec<PathBuf>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let front = ResourcePath::self_field("front");
    let back = ResourcePath::self_field("back");

    let program = ProgramIr {
        crate_name: "demo".into(),
        functions: vec![
            FunctionIr {
                name: "push".into(),
                owner_type: Some("Queue".into()),
                signature: FunctionSignature {
                    self_access: Some(AccessKind::MutBorrow),
                    generic_params: vec!["T".into()],
                },
                blocks: 1,
                loops: vec![],
                successors: vec![],
                events: vec![Event::Call(CallEvent {
                    block: 0,
                    callee: "alloc::vec::Vec::push".into(),
                    method: "push".into(),
                    receiver: Some(back.clone()),
                    receiver_ty: Some("alloc::vec::Vec<T>".into()),
                    receiver_access: AccessKind::MutBorrow,
                    args: vec![],
                    is_trait_call: false,
                    destination: None,
                })],
            },
            FunctionIr {
                name: "pop".into(),
                owner_type: Some("Queue".into()),
                signature: FunctionSignature {
                    self_access: Some(AccessKind::MutBorrow),
                    generic_params: vec!["T".into()],
                },
                blocks: 4,
                loops: vec![LoopRegion { blocks: vec![1, 2] }],
                successors: vec![],
                events: vec![
                    Event::Call(CallEvent {
                        block: 0,
                        callee: "alloc::vec::Vec::is_empty".into(),
                        method: "is_empty".into(),
                        receiver: Some(front.clone()),
                        receiver_ty: Some("alloc::vec::Vec<T>".into()),
                        receiver_access: AccessKind::SharedBorrow,
                        args: vec![],
                        is_trait_call: false,
                        destination: None,
                    }),
                    Event::Call(CallEvent {
                        block: 1,
                        callee: "alloc::vec::Vec::pop".into(),
                        method: "pop".into(),
                        receiver: Some(back.clone()),
                        receiver_ty: Some("alloc::vec::Vec<T>".into()),
                        receiver_access: AccessKind::MutBorrow,
                        args: vec![],
                        is_trait_call: false,
                        destination: None,
                    }),
                    Event::Call(CallEvent {
                        block: 2,
                        callee: "alloc::vec::Vec::push".into(),
                        method: "push".into(),
                        receiver: Some(front.clone()),
                        receiver_ty: Some("alloc::vec::Vec<T>".into()),
                        receiver_access: AccessKind::MutBorrow,
                        args: vec![],
                        is_trait_call: false,
                        destination: None,
                    }),
                    Event::Call(CallEvent {
                        block: 3,
                        callee: "alloc::vec::Vec::pop".into(),
                        method: "pop".into(),
                        receiver: Some(front),
                        receiver_ty: Some("alloc::vec::Vec<T>".into()),
                        receiver_access: AccessKind::MutBorrow,
                        args: vec![],
                        is_trait_call: false,
                        destination: None,
                    }),
                ],
            },
        ],
    };

    let mut summaries = SummaryDb::trusted_std();
    for file in user_summary_files {
        let txt = std::fs::read_to_string(&file)?;
        let user_db = SummaryDb::from_toml(&txt)?;
        summaries = summaries.merge(user_db);
    }

    let solver = GoodLpHighsSolver;
    let mut opts = AnalysisOptions::default();
    opts.summary_mode = summary_mode;
    let report = analyze_program("demo", &program, &summaries, &solver, &opts);

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        let style = TerminalStyle::auto();
        for m in report.methods {
            let method = style.bold(m.method.as_str());
            let bound = style.cyan(m.amortized_bound.as_str());
            let potential = style.magenta(describe_potential(m.potential.as_deref()));
            let status = format_status(&m.status, style);

            println!("- {method}: {bound} with {potential} ({status})");
            for d in m.diagnostics {
                let level = format_diagnostic_level(d.level.as_str(), style);
                println!("  - {level}: {}", d.message);
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct TerminalStyle {
    enabled: bool,
}

impl TerminalStyle {
    fn auto() -> Self {
        let no_color = std::env::var_os("NO_COLOR").is_some();
        let force_color = std::env::var("CLICOLOR_FORCE")
            .map(|value| value != "0")
            .unwrap_or(false);
        let color_disabled = std::env::var("CLICOLOR")
            .map(|value| value == "0")
            .unwrap_or(false);
        let dumb_terminal = std::env::var("TERM")
            .map(|value| value == "dumb")
            .unwrap_or(false);

        Self {
            enabled: force_color
                || (!no_color
                    && !color_disabled
                    && !dumb_terminal
                    && std::io::stdout().is_terminal()),
        }
    }

    fn paint(self, code: &str, text: impl AsRef<str>) -> String {
        let text = text.as_ref();
        if self.enabled {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    fn bold(self, text: impl AsRef<str>) -> String {
        self.paint("1", text)
    }

    fn cyan(self, text: impl AsRef<str>) -> String {
        self.paint("36;1", text)
    }

    fn green(self, text: impl AsRef<str>) -> String {
        self.paint("32;1", text)
    }

    fn magenta(self, text: impl AsRef<str>) -> String {
        self.paint("35;1", text)
    }

    fn red(self, text: impl AsRef<str>) -> String {
        self.paint("31;1", text)
    }

    fn yellow(self, text: impl AsRef<str>) -> String {
        self.paint("33;1", text)
    }
}

fn format_status(status: &Status, style: TerminalStyle) -> String {
    match status {
        Status::Proven => style.green("proven"),
        Status::Partial => style.yellow("partial"),
        Status::Undefined => style.red("undefined"),
    }
}

fn format_diagnostic_level(level: &str, style: TerminalStyle) -> String {
    match level {
        "error" => style.red(level),
        "warn" => style.yellow(level),
        "info" => style.cyan(level),
        _ => style.bold(level),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PotentialTerm {
    coeff: i64,
    path: String,
}

fn describe_potential(potential: Option<&str>) -> String {
    let Some(potential) = potential.map(str::trim).filter(|p| !p.is_empty()) else {
        return "no potential inferred".to_string();
    };

    if potential.replace(' ', "") == "0" {
        return "zero potential".to_string();
    }

    let Some(terms) = parse_potential_terms(potential) else {
        return format!("potential {}", normalize_potential_expression(potential));
    };

    let nonzero_terms = terms
        .iter()
        .filter(|term| term.coeff != 0)
        .collect::<Vec<_>>();

    if nonzero_terms.is_empty() {
        let paths = terms
            .iter()
            .map(|term| short_resource_name(term.path.as_str()))
            .collect::<Vec<_>>();

        if paths.is_empty() {
            "zero potential".to_string()
        } else {
            format!("zero potential on {}", join_words(&paths))
        }
    } else {
        let expression = nonzero_terms
            .iter()
            .map(|term| format_potential_term(term.coeff, term.path.as_str()))
            .collect::<Vec<_>>()
            .join(" + ")
            .replace("+ -", "- ");
        format!("potential {expression}")
    }
}

fn parse_potential_terms(potential: &str) -> Option<Vec<PotentialTerm>> {
    potential
        .split('+')
        .map(parse_potential_term)
        .collect::<Option<Vec<_>>>()
}

fn parse_potential_term(term: &str) -> Option<PotentialTerm> {
    let compact = term.trim().replace(' ', "");
    if compact.is_empty() {
        return None;
    }

    let (coeff, path_with_suffix) = if let Some(path) = compact.strip_prefix("len(") {
        (1, path)
    } else {
        let (coeff, path) = compact.split_once("*len(")?;
        (coeff.parse::<i64>().ok()?, path)
    };

    let path = path_with_suffix.strip_suffix(')')?;
    Some(PotentialTerm {
        coeff,
        path: path.to_string(),
    })
}

fn normalize_potential_expression(potential: &str) -> String {
    potential.replace('*', " * ")
}

fn format_potential_term(coeff: i64, path: &str) -> String {
    match coeff {
        1 => format!("len({path})"),
        -1 => format!("-len({path})"),
        coeff => format!("{coeff} * len({path})"),
    }
}

fn short_resource_name(path: &str) -> String {
    path.strip_prefix("self.").unwrap_or(path).to_string()
}

fn join_words(words: &[String]) -> String {
    match words {
        [] => String::new(),
        [one] => one.clone(),
        [first, second] => format!("{first} and {second}"),
        many => {
            let (last, head) = many.split_last().expect("non-empty words");
            format!("{}, and {last}", head.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_analyze_demo_summary_mode_and_summary_files() {
        let cli = Cli::try_parse_from([
            "cargo-ramort",
            "analyze-demo",
            "--json",
            "--summary-mode",
            "none",
            "--summaries",
            "summaries/local.toml",
        ])
        .expect("valid analyze-demo args should parse");

        match cli.cmd {
            Command::AnalyzeDemo {
                json,
                summary_mode,
                summaries,
            } => {
                assert!(json);
                assert_eq!(summary_mode, SummaryMode::None);
                assert_eq!(summaries, vec![PathBuf::from("summaries/local.toml")]);
            }
            other => panic!("expected analyze-demo command, got {other:?}"),
        }
    }

    #[test]
    fn parses_cargo_plan_package_and_repeated_features() {
        let cli = Cli::try_parse_from([
            "cargo-ramort",
            "cargo-plan",
            "--package",
            "demo",
            "--features",
            "a",
            "--features",
            "b",
        ])
        .expect("valid cargo-plan args should parse");

        match cli.cmd {
            Command::CargoPlan { package, features } => {
                assert_eq!(package.as_deref(), Some("demo"));
                assert_eq!(features, vec!["a", "b"]);
            }
            other => panic!("expected cargo-plan command, got {other:?}"),
        }
    }

    #[test]
    fn describes_zero_potential_on_single_resource() {
        assert_eq!(
            describe_potential(Some("0*len(self.back)")),
            "zero potential on back"
        );
    }

    #[test]
    fn omits_zero_terms_from_nonzero_potential() {
        assert_eq!(
            describe_potential(Some("0*len(self.front) + 2*len(self.back)")),
            "potential 2 * len(self.back)"
        );
    }
}

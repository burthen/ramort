#![feature(rustc_private)]

use clap::{Args as ClapArgs, Parser, Subcommand};
use ramort_core::{
    analyze_program, AnalysisOptions, AnalysisReport, MethodReport, Status, SummaryDb, SummaryMode,
};
use ramort_solver_goodlp::GoodLpHighsSolver;
use std::io::IsTerminal;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "ramort-rustc")]
#[command(about = "Nightly rustc frontend for RAMORT")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Backward-compatible shorthand for `dump-ir <FILE>`.
    file: Option<PathBuf>,

    #[arg(long = "rustc-arg", allow_hyphen_values = true)]
    rustc_args: Vec<String>,
}

#[derive(Subcommand, Debug)]
enum Command {
    DumpIr(SourceArgs),
    AnalyzeFile(AnalyzeFileArgs),
}

#[derive(ClapArgs, Debug)]
struct SourceArgs {
    file: PathBuf,

    #[arg(long = "rustc-arg", allow_hyphen_values = true)]
    rustc_args: Vec<String>,
}

#[derive(ClapArgs, Debug)]
struct AnalyzeFileArgs {
    file: PathBuf,

    #[arg(long)]
    json: bool,

    #[arg(long, default_value = "trusted-std")]
    summary_mode: SummaryMode,

    #[arg(long)]
    summaries: Vec<PathBuf>,

    #[arg(long = "rustc-arg", allow_hyphen_values = true)]
    rustc_args: Vec<String>,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::DumpIr(args)) => dump_ir(&args.file, &args.rustc_args),
        Some(Command::AnalyzeFile(args)) => analyze_file(args),
        None => {
            let file = cli
                .file
                .ok_or_else(|| "missing input file; use `ramort-rustc dump-ir <FILE>` or `ramort-rustc analyze-file <FILE>`".to_string())?;
            dump_ir(&file, &cli.rustc_args)
        }
    }
}

fn dump_ir(file: &PathBuf, rustc_args: &[String]) -> Result<(), String> {
    let ir = ramort_frontend_rustc::collect_mir_ir(file, rustc_args)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&ir).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn analyze_file(args: AnalyzeFileArgs) -> Result<(), String> {
    let ir = ramort_frontend_rustc::collect_mir_ir(&args.file, &args.rustc_args)?;

    let mut summaries = SummaryDb::trusted_std();
    for file in &args.summaries {
        let txt = std::fs::read_to_string(file)
            .map_err(|e| format!("failed to read summary file {}: {e}", file.display()))?;
        let user_db = SummaryDb::from_toml(&txt)
            .map_err(|e| format!("failed to parse summary file {}: {e}", file.display()))?;
        summaries = summaries.merge(user_db);
    }

    let solver = GoodLpHighsSolver;
    let mut opts = AnalysisOptions::default();
    opts.summary_mode = args.summary_mode;
    let report = analyze_program(
        args.file.display().to_string(),
        &ir,
        &summaries,
        &solver,
        &opts,
    );

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
    } else {
        print_human_report(&report);
    }

    Ok(())
}

fn print_human_report(report: &AnalysisReport) {
    let style = TerminalStyle::auto();
    let counts = ReportCounts::from_report(report);
    let method_width = report
        .methods
        .iter()
        .map(|method| method.method.len())
        .max()
        .unwrap_or(6)
        .clamp(6, 40);

    println!(
        "{} {}",
        style.bold("RAMORT analysis:"),
        style.dim(report.file.as_str())
    );
    println!(
        "{} {}  {}  {}",
        style.dim("summary:"),
        style.green(format!("{} proven", counts.proven)),
        style.yellow(format!("{} partial", counts.partial)),
        style.red(format!("{} undefined", counts.undefined)),
    );

    if report.methods.is_empty() {
        println!();
        println!("{}", style.yellow("No functions were collected."));
        return;
    }

    println!();
    for (idx, method) in report.methods.iter().enumerate() {
        if idx > 0 {
            println!();
        }
        print_method(method, method_width, style);
    }
}

fn print_method(method: &MethodReport, method_width: usize, style: TerminalStyle) {
    let method_name = format!("{:<method_width$}", method.method);
    println!(
        "{} {} {}",
        format_status_badge(&method.status, style),
        style.bold(method_name),
        style.cyan(method.amortized_bound.as_str()),
    );

    println!(
        "  {} {}",
        style.dim("potential:"),
        describe_potential(method.potential.as_deref())
    );

    if !method.assumptions.is_empty() {
        for assumption in &method.assumptions {
            println!("  {} {}", style.dim("reason:   "), assumption);
        }
    }

    if !method.obligations.is_empty() {
        let proven = method.obligations.iter().filter(|o| o.check.proven).count();
        let total = method.obligations.len();
        println!(
            "  {} {}",
            style.dim("proof:    "),
            format!("{proven}/{total} obligations checked")
        );
        for obligation in &method.obligations {
            let status = if obligation.check.proven {
                style.green("ok")
            } else {
                style.red("failed")
            };
            println!(
                "    {} {:<12} {}",
                status,
                obligation.obligation.name,
                style.dim(obligation.obligation.explanation.as_str())
            );
        }
    }

    if !method.diagnostics.is_empty() {
        println!("  {}", style.dim("diagnostics"));
        for diagnostic in &method.diagnostics {
            println!(
                "    {} {}",
                format_diagnostic_level(diagnostic.level.as_str(), style),
                diagnostic.message
            );
        }
    }
}

#[derive(Default)]
struct ReportCounts {
    proven: usize,
    partial: usize,
    undefined: usize,
}

impl ReportCounts {
    fn from_report(report: &AnalysisReport) -> Self {
        let mut counts = Self::default();
        for method in &report.methods {
            match method.status {
                Status::Proven => counts.proven += 1,
                Status::Partial => counts.partial += 1,
                Status::Undefined => counts.undefined += 1,
            }
        }
        counts
    }
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

    fn dim(self, text: impl AsRef<str>) -> String {
        self.paint("2", text)
    }

    fn green(self, text: impl AsRef<str>) -> String {
        self.paint("32;1", text)
    }

    fn red(self, text: impl AsRef<str>) -> String {
        self.paint("31;1", text)
    }

    fn yellow(self, text: impl AsRef<str>) -> String {
        self.paint("33;1", text)
    }
}

fn format_status_badge(status: &Status, style: TerminalStyle) -> String {
    match status {
        Status::Proven => style.green("[proven]"),
        Status::Partial => style.yellow("[partial]"),
        Status::Undefined => style.red("[undefined]"),
    }
}

fn format_diagnostic_level(level: &str, style: TerminalStyle) -> String {
    match level {
        "error" => style.red("error"),
        "warn" => style.yellow("warn "),
        "info" => style.cyan("info "),
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
        return "none inferred".to_string();
    };

    if potential.replace(' ', "") == "0" {
        return "zero".to_string();
    }

    let Some(terms) = parse_potential_terms(potential) else {
        return normalize_potential_expression(potential);
    };

    let nonzero_terms = terms
        .iter()
        .filter(|term| term.coeff != 0)
        .collect::<Vec<_>>();

    if nonzero_terms.is_empty() {
        return "zero".to_string();
    }

    nonzero_terms
        .iter()
        .map(|term| format_potential_term(term.coeff, term.path.as_str()))
        .collect::<Vec<_>>()
        .join(" + ")
        .replace("+ -", "- ")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_input_file_and_repeated_rustc_args() {
        let cli = Cli::try_parse_from([
            "ramort-rustc",
            "tests/mir/alias_cases.rs",
            "--rustc-arg",
            "--edition=2021",
            "--rustc-arg",
            "-Zmir-opt-level=0",
        ])
        .expect("valid frontend args should parse");

        assert_eq!(cli.file, Some(PathBuf::from("tests/mir/alias_cases.rs")));
        assert_eq!(
            cli.rustc_args,
            vec![
                "--edition=2021".to_string(),
                "-Zmir-opt-level=0".to_string()
            ]
        );
    }

    #[test]
    fn parses_analyze_file_args() {
        let cli = Cli::try_parse_from([
            "ramort-rustc",
            "analyze-file",
            "examples/queue.rs",
            "--json",
            "--summary-mode",
            "all",
            "--summaries",
            "summaries/project.toml",
            "--rustc-arg",
            "--crate-type=lib",
        ])
        .expect("valid analyze-file args should parse");

        match cli.command {
            Some(Command::AnalyzeFile(args)) => {
                assert_eq!(args.file, PathBuf::from("examples/queue.rs"));
                assert!(args.json);
                assert_eq!(args.summary_mode, SummaryMode::All);
                assert_eq!(
                    args.summaries,
                    vec![PathBuf::from("summaries/project.toml")]
                );
                assert_eq!(args.rustc_args, vec!["--crate-type=lib"]);
            }
            other => panic!("expected analyze-file command, got {other:?}"),
        }
    }
}

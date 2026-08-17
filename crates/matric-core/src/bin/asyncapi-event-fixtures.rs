use std::path::PathBuf;
use std::process::ExitCode;

use matric_core::asyncapi_event_fixtures::{check_fixture_corpus, generate_fixture_corpus};

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let Some(command) = args.next().and_then(|arg| arg.into_string().ok()) else {
        eprintln!("usage: asyncapi-event-fixtures generate|check [repo-root]");
        return ExitCode::from(2);
    };
    let root = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    if args.next().is_some() {
        eprintln!("usage: asyncapi-event-fixtures generate|check [repo-root]");
        return ExitCode::from(2);
    }

    let result = match command.as_str() {
        "generate" => generate_fixture_corpus(&root),
        "check" => check_fixture_corpus(&root),
        _ => {
            eprintln!("usage: asyncapi-event-fixtures generate|check [repo-root]");
            return ExitCode::from(2);
        }
    };

    match result {
        Ok(summary) => {
            println!(
                "{} event fixtures verified; corpus sha256={}",
                summary.event_count, summary.corpus_sha256
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

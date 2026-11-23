use composer_cli::execute;
use composer_cli::plan::{plan, Input, UsageError, USAGE};
use composer_cli::transport::{StdioTransport, Transport, WsTransport};
use std::io::Read;
use std::process::ExitCode;

struct RealInput;

impl Input for RealInput {
    fn file(&self, path: &str) -> Result<String, UsageError> {
        std::fs::read_to_string(path).map_err(|e| UsageError(format!("{path}: {e}")))
    }
    fn stdin(&self) -> Result<String, UsageError> {
        let mut s = String::new();
        std::io::stdin()
            .read_to_string(&mut s)
            .map_err(|e| UsageError(format!("stdin: {e}")))?;
        Ok(s)
    }
}

fn fail(msg: &str, code: u8) -> ExitCode {
    eprintln!("composer: {msg}");
    ExitCode::from(code)
}

fn main() -> ExitCode {
    let mut daemon = std::env::var("COMPOSER_DAEMON").unwrap_or_else(|_| "composerd".into());
    let mut ws = std::env::var("COMPOSER_WS_URL").ok();
    let (mut dry, mut keep_going) = (false, false);
    let mut words = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--daemon" => match args.next() {
                Some(v) => daemon = v,
                None => return fail("--daemon needs a path", 2),
            },
            "--ws" => match args.next() {
                Some(v) => ws = Some(v),
                None => return fail("--ws needs a URL", 2),
            },
            "--dry-run" => dry = true,
            "--keep-going" => keep_going = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            _ => {
                words.push(a);
                words.extend(args.by_ref());
            }
        }
    }
    let steps = match plan(&words, dry, &RealInput) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("composer: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let mut t: Box<dyn Transport> = match ws {
        Some(url) => {
            let Ok(token) = std::env::var("COMPOSER_TOKEN") else {
                return fail("--ws needs COMPOSER_TOKEN in the environment", 2);
            };
            match WsTransport::connect(&url, &token) {
                Ok(t) => Box::new(t),
                Err(e) => return fail(&e.to_string(), 3),
            }
        }
        None => match StdioTransport::spawn(&daemon) {
            Ok(t) => Box::new(t),
            Err(e) => return fail(&e.to_string(), 3),
        },
    };
    match execute(t.as_mut(), &steps, keep_going) {
        Ok((out, ok)) => {
            for v in out {
                println!("{v}");
            }
            if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(e) => fail(&e.to_string(), 3),
    }
}

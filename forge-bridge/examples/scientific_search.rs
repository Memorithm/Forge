//! Strict bounded stdin/stdout process protocol for scientific ask/tell.
//! Run with a Request JSON on stdin; errors exit 21 without partial stdout.

use forge_bridge::scientific_ask_tell::{
    handle, Checkpoint, Command, Request, SearchSession, SearchSpec,
};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::fmt;
use std::io::{self, BufRead, Read, Write};

struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Unique;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("strict JSON without duplicate keys")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Unique, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| Unique(Value::Number(n)))
                    .ok_or_else(|| E::custom("non-finite number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut v = Vec::new();
                while let Some(Unique(x)) = a.next_element()? {
                    v.push(x);
                }
                Ok(Unique(Value::Array(v)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut v = serde_json::Map::new();
                while let Some((k, Unique(x))) = a.next_entry::<String, Unique>()? {
                    if v.insert(k, x).is_some() {
                        return Err(de::Error::custom("duplicate JSON key"));
                    }
                }
                Ok(Unique(Value::Object(v)))
            }
        }
        d.deserialize_any(V)
    }
}

fn parse<T: serde::de::DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T, String> {
    let Unique(value) = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let request: T = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    // Existing external-domain types intentionally accept extensions. This v1
    // wire protocol is closed, including those nested administrative records.
    if serde_json::to_value(&request).map_err(|e| e.to_string())? != value {
        return Err("unknown or omitted fields in closed search protocol".into());
    }
    Ok(request)
}
fn run() -> Result<(), String> {
    let mut bytes = Vec::new();
    io::stdin()
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("search input exceeds 4 MiB".into());
    }
    let result = handle(parse::<Request>(&bytes)?)?;
    let output = serde_json::to_vec(&result).map_err(|e| e.to_string())?;
    if output.len() > 8 * 1024 * 1024 {
        return Err("search output exceeds 8 MiB".into());
    }
    io::stdout()
        .lock()
        .write_all(&output)
        .map_err(|e| e.to_string())
}

const SESSION_PROTOCOL: &str = "forge-scientific-session/v1";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Frame {
    protocol: String,
    action: Action,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
enum Action {
    Open {
        spec: Box<SearchSpec>,
        checkpoint: Option<Checkpoint>,
    },
    Command {
        spec_sha256: String,
        expected_sequence: usize,
        command: Command,
    },
    Inspect {
        spec_sha256: String,
        expected_sequence: usize,
    },
}

fn run_session() -> Result<(), String> {
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let mut session: Option<SearchSession> = None;
    // Inspections consume frames too; a process never accepts an unbounded log.
    for _ in 0..4098 {
        let mut raw = Vec::new();
        let n = input
            .by_ref()
            .take(4 * 1024 * 1024 + 1)
            .read_until(b'\n', &mut raw)
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return if session.is_some() {
                Ok(())
            } else {
                Err("session requires open".into())
            };
        }
        if n > 4 * 1024 * 1024 || raw.last() != Some(&b'\n') {
            return Err("oversized or unterminated session frame".into());
        }
        let frame: Frame = parse(&raw)?;
        if frame.protocol != SESSION_PROTOCOL {
            return Err("unsupported session protocol".into());
        }
        let result = match frame.action {
            Action::Open { spec, checkpoint } => {
                if session.is_some() {
                    return Err("session already open".into());
                }
                let mut opened = SearchSession::restore(*spec, checkpoint)?;
                let result = serde_json::json!({"kind":"opened", "response":opened.response()});
                session = Some(opened);
                result
            }
            Action::Command {
                spec_sha256,
                expected_sequence,
                command,
            } => {
                let active = session.as_mut().ok_or("session requires open")?;
                let receipt = active.submit(&spec_sha256, expected_sequence, command)?;
                serde_json::json!({"kind":"receipt", "spec_sha256":active.spec_sha256(),
                                  "sequence":active.sequence(), "receipt":receipt})
            }
            Action::Inspect {
                spec_sha256,
                expected_sequence,
            } => {
                let active = session.as_mut().ok_or("session requires open")?;
                active.check_position(&spec_sha256, expected_sequence)?;
                serde_json::json!({"kind":"snapshot", "response":active.response()})
            }
        };
        let bytes =
            serde_json::to_vec(&serde_json::json!({"protocol":SESSION_PROTOCOL, "result":result}))
                .map_err(|e| e.to_string())?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err("session output exceeds 8 MiB".into());
        }
        output
            .write_all(&bytes)
            .and_then(|_| output.write_all(b"\n"))
            .and_then(|_| output.flush())
            .map_err(|e| e.to_string())?;
    }
    // The last allowed reply is valid. EOF after it is a clean close; any
    // additional byte exceeds the frame budget and must not be processed.
    let mut extra = [0u8; 1];
    if input.read(&mut extra).map_err(|e| e.to_string())? == 0 {
        Ok(())
    } else {
        Err("session frame budget exhausted".into())
    }
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [] => run(),
        [arg] if arg == "--session" => run_session(),
        _ => Err("expected no arguments or --session".into()),
    };
    if let Err(error) = result {
        eprintln!("scientific search contract: {error}");
        std::process::exit(21);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_keys_at_any_depth_overflow_nonfinite_and_deep_json_fail() {
        for raw in [
            r#"{"spec":{},"spec":{}}"#,
            r#"{"spec":{"seed":1,"seed":2}}"#,
            r#"{"spec":{"seed":1e999}}"#,
            "NaN",
        ] {
            assert!(parse::<Request>(raw.as_bytes()).is_err());
        }
        let deep = format!("{}0{}", "[".repeat(130), "]".repeat(130));
        assert!(parse::<Request>(deep.as_bytes()).is_err());
    }
}

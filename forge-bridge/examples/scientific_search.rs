//! Strict bounded stdin/stdout process protocol for scientific ask/tell.
//! Run with a Request JSON on stdin; errors exit 21 without partial stdout.

use forge_bridge::scientific_ask_tell::{handle, Request};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use std::fmt;
use std::io::{self, Read};

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

fn parse(bytes: &[u8]) -> Result<Request, String> {
    let Unique(value) = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let request: Request = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
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
    let result = handle(parse(&bytes)?)?;
    let output = serde_json::to_vec(&result).map_err(|e| e.to_string())?;
    if output.len() > 8 * 1024 * 1024 {
        return Err("search output exceeds 8 MiB".into());
    }
    use std::io::Write;
    io::stdout()
        .lock()
        .write_all(&output)
        .map_err(|e| e.to_string())
}
fn main() {
    if let Err(error) = run() {
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
            assert!(parse(raw.as_bytes()).is_err());
        }
        let deep = format!("{}0{}", "[".repeat(130), "]".repeat(130));
        assert!(parse(deep.as_bytes()).is_err());
    }
}

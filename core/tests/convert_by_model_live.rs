// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! How well a model on a server converts a document, measured by SCE's own check and not by the model's
//! word (ADR 0026, 0036).
//!
//! Ignored by default, because it runs a model: a document is cut into units, each unit goes to the
//! server one at a time, and the answers are put together and counted. It needs a server that speaks
//! Chat Completions (llama.cpp's, Ollama, vLLM, LM Studio), `python3` with PyYAML, and the `sce`
//! checkout's authoring tools. Nothing here is a product path: the host that imports a document by a model
//! is a later step, and this holds the model's part of it to account first.
//!
//! The server is named by `SCE_LOCAL_URL` (`http://127.0.0.1:18081/v1`) and the model by
//! `SCE_LOCAL_MODEL`; a key the server wants by `SCE_LOCAL_KEY`. The documents are named by
//! `SCE_LIVE_DOCS`, paths separated by `:`; by default three documents of the `sce` checkout, which is
//! public.
//!
//! **What is printed is counts and ids, never a document's words.** That is so that the same run can be
//! made of a document that is not public. A document that must not leave the machine is for a server on
//! the person's own network and no other, and that is for its owner to say, not for this test to decide.
//!
//! ```text
//! SCE_LOCAL_URL=http://127.0.0.1:18081/v1 SCE_LOCAL_MODEL=qwen3.8-flash-next-iq2_xs \
//!   cargo test -p sce-spec-workbench-core --test convert_by_model_live -- --ignored --nocapture
//! ```

#![cfg(unix)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use sce_spec_workbench_core::http_client::{send, Endpoint, Request};
use sce_spec_workbench_core::runner::Cancel;
use serde_json::{json, Value};

/// How long one unit may take. A model that is small and slow is given time; one that hangs is not
/// waited for forever.
const UNIT_DEADLINE: Duration = Duration::from_secs(20 * 60);
const MOST_ANSWER: usize = 4 * 1024 * 1024;

fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is required: see the top of this file"))
}

fn tools() -> PathBuf {
    let launcher = common::sce_root().join("scripts").join("sce_author.sh");
    assert!(
        launcher.is_file(),
        "{} is not in the checkout",
        launcher.display()
    );
    launcher
}

/// Run the authoring tools with `args` in `dir` and give back the exit code and what they printed.
fn tool(dir: &Path, args: &[&str]) -> (i32, String, String) {
    let ran = Command::new(tools())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("the authoring tools could not be started");
    (
        ran.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&ran.stdout).into_owned(),
        String::from_utf8_lossy(&ran.stderr).into_owned(),
    )
}

/// Ask the model one thing, with nothing else in the conversation.
fn ask(
    endpoint: &Endpoint,
    bearer: Option<&str>,
    model: &str,
    prompt: &str,
) -> Result<String, String> {
    let body = serde_json::to_vec(&json!({
        "model": model,
        "messages": [{ "role": "user", "content": prompt }],
        "temperature": 0,
        "stream": false,
    }))
    .map_err(|e| e.to_string())?;
    let answer = send(
        endpoint,
        Request {
            method: "POST",
            path: "/chat/completions",
            bearer,
            json: Some(&body),
        },
        Instant::now() + UNIT_DEADLINE,
        &Cancel::new(),
        MOST_ANSWER,
    )
    .map_err(|e| e.to_string())?;
    if answer.status != 200 {
        return Err(format!("the server answered {}", answer.status));
    }
    let said: Value = serde_json::from_slice(&answer.body).map_err(|e| format!("not JSON: {e}"))?;
    said["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "the answer has no message content".to_string())
}

/// What one document came to.
struct Measure {
    units: usize,
    first_try_failed: usize,
    still_failed: usize,
    asked: usize,
    seconds: u64,
    matched: u64,
    check: usize,
    not_in_original: usize,
    missing: usize,
    complete: bool,
}

fn convert(
    endpoint: &Endpoint,
    bearer: Option<&str>,
    model: &str,
    document: &Path,
    work: &Path,
) -> Measure {
    let started = Instant::now();
    let name = document
        .file_name()
        .and_then(|n| n.to_str())
        .expect("a document has a name");
    fs::copy(document, work.join(name)).expect("the document could not be copied");
    let (code, _, why) = tool(work, &["extract", name, "--out", "e.json"]);
    assert_eq!(code, 0, "extract refused: {why}");
    let (code, _, why) = tool(
        work,
        &[
            "prepare-conversion",
            "--extraction",
            "e.json",
            "--out-dir",
            "units",
        ],
    );
    assert_eq!(code, 0, "prepare-conversion refused: {why}");

    let listing: Value =
        serde_json::from_str(&fs::read_to_string(work.join("units/units.json")).unwrap()).unwrap();
    let units: Vec<u64> = listing["units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["n"].as_u64().unwrap())
        .collect();

    let mut asked = 0;
    let mut answer_unit = |n: u64| {
        let prompt =
            fs::read_to_string(work.join(format!("units/unit-{n:04}.prompt.txt"))).unwrap();
        asked += 1;
        // A server that did not answer is not an answer: the unit is left without one and fails as such.
        match ask(endpoint, bearer, model, &prompt) {
            Ok(text) => {
                fs::write(work.join(format!("units/unit-{n:04}.answer.txt")), text).unwrap()
            }
            Err(_) => {
                let _ = fs::remove_file(work.join(format!("units/unit-{n:04}.answer.txt")));
            }
        }
    };
    for n in &units {
        answer_unit(*n);
    }

    let finish = |work: &Path| {
        let (code, out, why) = tool(
            work,
            &[
                "finish-conversion",
                "--extraction",
                "e.json",
                "--dir",
                "units",
                "--converter-name",
                "measured",
                "--out",
                "c.json",
                "--json",
            ],
        );
        (code, out, why)
    };
    let (code, out, why) = finish(work);
    let first_try_failed = failed_numbers(&out).len();
    // A unit that failed is asked again, once, as the loop will: the others are kept.
    let (out, why) = if code != 0 && !out.trim().is_empty() {
        for n in failed_numbers(&out) {
            answer_unit(n);
        }
        let (_, again, why) = finish(work);
        (again, why)
    } else {
        (out, why)
    };
    let still_failed = failed_numbers(&out).len();

    let (_, checked, check_why) = tool(
        work,
        &[
            "check-conversion",
            "--extraction",
            "e.json",
            "--conversion",
            "c.json",
            "--json",
        ],
    );
    let report: Value = serde_json::from_str(checked.trim()).unwrap_or_else(|_| {
        panic!("check-conversion did not print a report (the conversion may not have been written): {check_why} {why}")
    });
    let count = |key: &str| report[key].as_array().map_or(0, Vec::len);
    Measure {
        units: units.len(),
        first_try_failed,
        still_failed,
        asked,
        seconds: started.elapsed().as_secs(),
        matched: report["matched"].as_u64().unwrap_or(0),
        check: count("check"),
        not_in_original: count("not_in_original"),
        missing: count("missing"),
        complete: report["complete"].as_bool().unwrap_or(false),
    }
}

fn failed_numbers(out: &str) -> Vec<u64> {
    serde_json::from_str::<Value>(out.trim())
        .ok()
        .and_then(|r| r["failed"].as_array().cloned())
        .map(|list| list.iter().filter_map(|f| f["n"].as_u64()).collect())
        .unwrap_or_default()
}

fn documents() -> Vec<PathBuf> {
    match std::env::var("SCE_LIVE_DOCS") {
        Ok(list) if !list.trim().is_empty() => list.split(':').map(PathBuf::from).collect(),
        _ => {
            let adr = common::sce_root().join("docs").join("adr");
            let find = |prefix: &str| {
                fs::read_dir(&adr)
                    .expect("the checkout has docs/adr")
                    .filter_map(Result::ok)
                    .map(|e| e.path())
                    .find(|p| {
                        p.file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| n.starts_with(prefix))
                    })
                    .unwrap_or_else(|| panic!("no document of docs/adr begins {prefix}"))
            };
            vec![
                find("0026-"),
                find("0037-"),
                common::sce_root().join("SCE_WIRE_CONTRACTS.md"),
            ]
        }
    }
}

#[test]
#[ignore = "runs a model: it takes the server's time"]
fn a_model_on_a_server_converts_documents_and_sces_check_counts_what_it_did() {
    let url = required("SCE_LOCAL_URL");
    let model = required("SCE_LOCAL_MODEL");
    let key = std::env::var("SCE_LOCAL_KEY").ok();
    let endpoint = Endpoint::parse(&url).unwrap_or_else(|e| panic!("SCE_LOCAL_URL: {e}"));

    println!("model {model} at {url}");
    println!(
        "{:<4} {:>5} {:>6} {:>6} {:>7} {:>8} {:>5} {:>6} {:>7} {:>8}",
        "doc", "units", "retry", "fail", "asked", "seconds", "match", "check", "extra", "missing"
    );
    let mut all = Vec::new();
    for (index, document) in documents().iter().enumerate() {
        assert!(document.is_file(), "{} is not a file", document.display());
        let work = common::scratch(&format!("convert-live-{index}"));
        let measure = convert(&endpoint, key.as_deref(), &model, document, &work);
        println!(
            "{:<4} {:>5} {:>6} {:>6} {:>7} {:>8} {:>5} {:>6} {:>7} {:>8}{}",
            index + 1,
            measure.units,
            measure.first_try_failed,
            measure.still_failed,
            measure.asked,
            measure.seconds,
            measure.matched,
            measure.check,
            measure.not_in_original,
            measure.missing,
            if measure.complete { "  complete" } else { "" },
        );
        // A run that keeps its folder says where, for a document that is public. It is not kept
        // otherwise: what the model wrote is the document's words.
        if std::env::var_os("SCE_LIVE_KEEP").is_some() {
            println!("     kept: {}", work.display());
        } else {
            let _ = fs::remove_dir_all(&work);
        }
        all.push(measure);
    }
    // What is held is that the measurement was made, not how well the model did: a model that does badly
    // is the finding, and a run that could not be counted is the failure.
    assert!(all.iter().all(|m| m.units > 0), "a document had no unit");
}

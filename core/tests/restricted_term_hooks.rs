// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! The workbench's own hooks keep the restricted-term list out of its commits.
//!
//! This repository is public and a push does not un-publish, so what a commit carries (its files and
//! its message) is held to the list at the commit and not only at the push. The hooks are
//! `scripts/git-hooks/`, and the gate they run is SCE's, found in the `sce` submodule. What is held
//! here is the wiring: that a real `git commit` reaches them (git passes over a hook without the
//! executable bit and says nothing), that a staged term and a term in the message each refuse the
//! commit, that nothing printed is the matched text, and that a checkout without the gate refuses
//! rather than passes.
//!
//! The list is a synthetic one, given through `SCE_RESTRICTED_TERMS`. The real list is never
//! opened by a test: reading it would copy the terms it exists to keep out.

#![cfg(unix)]

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const TERMS: &str = "# a synthetic list for this suite\nPROBE_TERM_[0-9]+\n";

fn hooks() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the core is a folder of the repository")
        .join("scripts/git-hooks")
}

/// A repository of its own, with the hooks switched on the way `install.sh` does it (but by an
/// absolute path, because this repository is not the one under test), and `sce` carrying the gate
/// or not.
struct Sandbox {
    dir: PathBuf,
    terms: PathBuf,
}

impl Sandbox {
    fn new(label: &str, with_gate: bool) -> Sandbox {
        let dir = common::scratch(label);
        let sandbox = Sandbox {
            terms: dir.join("terms.list"),
            dir: dir.join("repo"),
        };
        std::fs::create_dir_all(&sandbox.dir).unwrap();
        std::fs::write(&sandbox.terms, TERMS).unwrap();
        sandbox.git(&["init", "-q"]);
        sandbox.git(&["config", "core.hooksPath", hooks().to_str().unwrap()]);
        if with_gate {
            let gate = sandbox.dir.join("sce/tools/git-hooks");
            std::fs::create_dir_all(&gate).unwrap();
            std::fs::copy(
                common::sce_root().join("tools/git-hooks/restricted-term-gate.sh"),
                gate.join("restricted-term-gate.sh"),
            )
            .expect("the sce submodule carries the gate");
        }
        sandbox
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(&self.dir)
            .args([
                "-c",
                "user.name=probe",
                "-c",
                "user.email=probe@example.invalid",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .env("SCE_RESTRICTED_TERMS", &self.terms)
            .env_remove("SCE_RESTRICTED_OVERRIDE");
        command
    }

    fn git(&self, args: &[&str]) {
        let out = self.command(args).output().expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?} failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn stage(&self, path: &str, text: &str) {
        std::fs::write(self.dir.join(path), text).unwrap();
        self.git(&["add", "--", path]);
    }

    fn commit(&self, message: &str) -> Output {
        self.command(&["commit", "-q", "-m", message])
            .output()
            .expect("git runs")
    }
}

fn said(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn a_commit_that_stages_a_term_is_refused_and_the_term_is_not_printed() {
    let sandbox = Sandbox::new("hooks-staged", true);
    sandbox.stage("notes.txt", "one\nthe term is PROBE_TERM_7 here\n");

    let out = sandbox.commit("a plain message");

    assert!(!out.status.success(), "{}", said(&out));
    let stderr = said(&out);
    assert!(
        stderr.contains("notes.txt: line(s) 2 — term #0"),
        "{stderr}"
    );
    assert!(!stderr.contains("PROBE_TERM_7"), "{stderr}");
    let log = sandbox.command(&["log", "--oneline"]).output().unwrap();
    assert!(!log.status.success(), "nothing was committed");
}

#[test]
fn a_commit_whose_message_carries_a_term_is_refused_and_a_clean_one_is_made() {
    let sandbox = Sandbox::new("hooks-message", true);
    sandbox.stage("notes.txt", "nothing restricted here\n");

    let out = sandbox.commit("fix: handle PROBE_TERM_12 in the reader");
    assert!(!out.status.success(), "{}", said(&out));
    let stderr = said(&out);
    assert!(
        stderr.contains("commit message: line(s) 1 — term #0"),
        "{stderr}"
    );
    assert!(!stderr.contains("PROBE_TERM_12"), "{stderr}");

    let made = sandbox.commit("fix: handle the reader");
    assert!(made.status.success(), "{}", said(&made));
}

#[test]
fn a_term_in_another_case_is_refused_on_an_added_line_and_in_the_message() {
    let sandbox = Sandbox::new("hooks-loose", true);
    sandbox.stage("notes.txt", "a line with probe_term_3 in lower case\n");
    let staged = sandbox.commit("a plain message");
    assert!(!staged.status.success(), "{}", said(&staged));
    assert!(
        said(&staged).contains("notes.txt: line(s) 1 — loose term #0"),
        "{}",
        said(&staged)
    );

    sandbox.stage("notes.txt", "a clean line\n");
    let message = sandbox.commit("fix: handle probe_term_4");
    assert!(!message.status.success(), "{}", said(&message));
    assert!(
        said(&message).contains("commit message: line(s) 1 — loose term #0"),
        "{}",
        said(&message)
    );
}

#[test]
fn a_checkout_without_the_gate_refuses_every_commit_instead_of_passing_it() {
    let sandbox = Sandbox::new("hooks-no-gate", false);
    sandbox.stage("notes.txt", "nothing restricted here\n");

    let out = sandbox.commit("a plain message");

    assert!(!out.status.success(), "no gate is no check");
    assert!(
        said(&out).contains("restricted-term gate is not in this checkout"),
        "{}",
        said(&out)
    );
}

#[test]
fn install_turns_the_hooks_on_for_the_clone_it_is_run_in() {
    let sandbox = Sandbox::new("hooks-install", true);
    sandbox.git(&["config", "--unset", "core.hooksPath"]);

    let out = Command::new("bash")
        .arg(hooks().join("install.sh"))
        .current_dir(&sandbox.dir)
        .env("SCE_RESTRICTED_TERMS", &sandbox.terms)
        .output()
        .expect("bash runs the installer");
    assert!(out.status.success(), "{}", said(&out));

    let set = sandbox
        .command(&["config", "core.hooksPath"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&set.stdout).trim(),
        "scripts/git-hooks"
    );
}

impl Sandbox {
    /// A commit made past the commit hooks, as `--no-verify` makes one (`post-commit` still runs).
    fn commit_past_the_hooks(&self, message: &str) -> Output {
        self.command(&["commit", "-q", "--no-verify", "-m", message])
            .output()
            .expect("git runs")
    }

    /// A bare repository to push to, set as `origin`.
    fn remote(&self) -> PathBuf {
        let bare = self.dir.parent().unwrap().join("remote.git");
        let _ = std::fs::remove_dir_all(&bare);
        let out = Command::new("git")
            .args(["init", "-q", "--bare"])
            .arg(&bare)
            .output()
            .expect("git runs");
        assert!(out.status.success());
        self.git(&["remote", "add", "origin", bare.to_str().unwrap()]);
        bare
    }

    fn push(&self) -> Output {
        self.command(&["push", "-q", "origin", "HEAD:refs/heads/main"])
            .output()
            .expect("git runs")
    }
}

#[test]
fn a_commit_made_past_the_hooks_is_warned_about_at_once_and_refused_at_the_push() {
    let sandbox = Sandbox::new("hooks-past", true);
    sandbox.remote();
    sandbox.stage("notes.txt", "nothing restricted\n");
    let clean = sandbox.commit_past_the_hooks("a plain message");
    assert!(clean.status.success(), "{}", said(&clean));
    assert!(!said(&clean).contains("REFUSED"), "{}", said(&clean));

    sandbox.stage(
        "notes.txt",
        "nothing restricted\nthe term is PROBE_TERM_9 here\n",
    );
    let past = sandbox.commit_past_the_hooks("a plain message");
    assert!(
        past.status.success(),
        "post-commit cannot undo the commit:\n{}",
        said(&past)
    );
    let warned = said(&past);
    assert!(
        warned.contains("notes.txt: line(s) 2 — term #0"),
        "{warned}"
    );
    assert!(warned.contains("would be REFUSED at the push"), "{warned}");
    assert!(!warned.contains("PROBE_TERM_9"), "{warned}");

    let pushed = sandbox.push();
    assert!(!pushed.status.success(), "the push refuses the commit");
    let refused = said(&pushed);
    assert!(
        refused.contains("notes.txt: line(s) 2 — term #0"),
        "{refused}"
    );
    assert!(!refused.contains("PROBE_TERM_9"), "{refused}");
    let at_remote = Command::new("git")
        .args(["--git-dir"])
        .arg(sandbox.dir.parent().unwrap().join("remote.git"))
        .args(["rev-parse", "--verify", "-q", "refs/heads/main"])
        .output()
        .unwrap();
    assert!(!at_remote.status.success(), "nothing reached the remote");

    // Taking the text out of the history lets the push through.
    sandbox.git(&["reset", "-q", "--hard", "HEAD~1"]);
    let clean_push = sandbox.push();
    assert!(clean_push.status.success(), "{}", said(&clean_push));
}

#[test]
fn a_checkout_without_the_gate_refuses_the_push_and_the_override_leaves_a_row() {
    let sandbox = Sandbox::new("hooks-push-no-gate", false);
    sandbox.remote();
    sandbox.stage("notes.txt", "nothing restricted\n");
    let made = sandbox.commit_past_the_hooks("a plain message");
    assert!(made.status.success(), "post-commit never fails a commit");

    let pushed = sandbox.push();
    assert!(!pushed.status.success(), "no gate is no check");
    assert!(
        said(&pushed).contains("restricted-term gate is not in this checkout"),
        "{}",
        said(&pushed)
    );

    // With the gate present, the override passes, says so, and is written down.
    let with_gate = Sandbox::new("hooks-push-override", true);
    with_gate.remote();
    with_gate.stage("notes.txt", "the term is PROBE_TERM_4 here\n");
    with_gate.commit_past_the_hooks("a plain message");
    let log = with_gate.dir.parent().unwrap().join("override.log");
    let out = with_gate
        .command(&["push", "-q", "origin", "HEAD:refs/heads/main"])
        .env("SCE_RESTRICTED_OVERRIDE", "1")
        .env("SCE_RESTRICTED_OVERRIDE_LOG", &log)
        .output()
        .expect("git runs");
    assert!(out.status.success(), "{}", said(&out));
    assert!(said(&out).contains("DISABLED"), "{}", said(&out));
    let rows = std::fs::read_to_string(&log).expect("the override was written down");
    assert!(rows.contains("\tpre-push\t"), "{rows}");
    assert!(!rows.contains("PROBE_TERM_4"), "{rows}");
}

#[test]
fn every_hook_of_the_repository_carries_the_executable_bit() {
    // git passes over a hook that is not executable and says nothing, which is a gate that is
    // off while it reads as on.
    use std::os::unix::fs::PermissionsExt;
    for entry in std::fs::read_dir(hooks()).unwrap() {
        let path = entry.unwrap().path();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert!(mode & 0o111 != 0, "{} is not executable", path.display());
    }
}

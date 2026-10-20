// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! What the workbench tests share: the stand-ins of `sce_specs::testing`, a folder of their own,
//! the product's programs a live test runs, and the stand-ins of a model server.

#![allow(dead_code)]

/// The stand-ins a model server run is held against (a shell script is one of them).
#[cfg(unix)]
pub mod model_server;

use std::path::{Path, PathBuf};

pub use sce_specs::testing::*;

/// A fresh, empty folder under the build's temporary directory, named for the test that asked
/// and unique to this run.
pub fn scratch(label: &str) -> PathBuf {
    scratch_in(Path::new(env!("CARGO_TARGET_TMPDIR")), label)
}

/// The SCE checkout the application is built against: the `sce` submodule of this repository.
/// The generator, `sce-spec` and the authoring server a live test runs are built and found there.
pub fn sce_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the core is a folder of the repository")
        .join("sce")
}

/// The `sce-spec` a live test hands its client: the one the environment names (`SCE_SPEC`), else
/// the debug build in the SCE checkout, which has to be there. It is the specs' command and not
/// this crate's, so it is not built for these tests.
pub fn sce_spec() -> PathBuf {
    let spec = std::env::var_os("SCE_SPEC")
        .map(PathBuf::from)
        .unwrap_or_else(|| sce_root().join("target/debug/sce-spec"));
    assert!(
        spec.is_file(),
        "{} is not built: cargo build --manifest-path sce/Cargo.toml -p sce-specs --features cli \
         --bin sce-spec, or name it with SCE_SPEC",
        spec.display()
    );
    spec
}

/// The picture of three lamp states in `fixtures/`, drawn from `lamp-states.svg` beside it.
pub fn lamp_states_png() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lamp-states.png")
}

/// A word document of a heading, a sentence, the lamp picture in a paragraph of its own, and a
/// caption, written by Python's `zipfile` into `dir`: no zip writer is in the tests, and the authoring
/// tools a test that reads one runs need Python already.
pub fn lamp_docx(dir: &Path) -> PathBuf {
    lamp_docx_with(dir, &lamp_states_png())
}

/// [`lamp_docx`] holding the picture at `png`: a contract file that records a reply wants a few bytes of
/// picture in it and not the drawing's thousands.
pub fn lamp_docx_with(dir: &Path, png: &Path) -> PathBuf {
    let out = dir.join("lamp.docx");
    let script = r#"
import sys, zipfile
out, png = sys.argv[1], sys.argv[2]
ns = ('xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" '
      'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" '
      'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"')
def p(text, style=None):
    props = f'<w:pPr><w:pStyle w:val="{style}"/></w:pPr>' if style else ''
    return f'<w:p>{props}<w:r><w:t>{text}</w:t></w:r></w:p>'
picture = ('<w:p><w:r><w:drawing><wp:inline xmlns:wp="x"><a:graphic><a:blip r:embed="rId5"/>'
           '</a:graphic></wp:inline></w:drawing></w:r></w:p>')
body = p('Lamp', 'Heading1') + p('The lamp is dark until it is switched on.') + picture + p('Figure 2. Lamp states')
rels = ('<?xml version="1.0" encoding="UTF-8"?><Relationships '
        'xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" '
        'Target="media/lamp-states.png"/></Relationships>')
with zipfile.ZipFile(out, 'w') as z:
    z.writestr('word/document.xml', f'<?xml version="1.0" encoding="UTF-8"?><w:document {ns}><w:body>{body}</w:body></w:document>')
    z.writestr('word/_rels/document.xml.rels', rels)
    z.write(png, 'word/media/lamp-states.png')
"#;
    let made = std::process::Command::new("python3")
        .args(["-I", "-c", script])
        .arg(&out)
        .arg(png)
        .status()
        .expect("python3 could not be started");
    assert!(made.success(), "the word document was not written");
    out
}

/// The authoring tools of the SCE checkout, by their launcher.
pub fn tools() -> PathBuf {
    let launcher = sce_root().join("scripts").join("sce_author.sh");
    assert!(
        launcher.is_file(),
        "{} is not in the checkout",
        launcher.display()
    );
    launcher
}

/// Run the authoring tools with `args` in `dir` and give back the exit code and what they printed.
pub fn tool(dir: &Path, args: &[&str]) -> (i32, String, String) {
    let ran = std::process::Command::new(tools())
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

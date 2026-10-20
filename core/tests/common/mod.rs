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

/// A PDF of one page: a line of text and, below it, a 2 by 2 colour image of raw samples, written byte by
/// byte into `dir`. Its image is a figure the authoring tools place and draw, and the PDF library they read
/// it with is a pack (`pypdfium2`) that a test which reads this file needs installed.
pub fn lamp_pdf(dir: &Path) -> PathBuf {
    let text = b"BT /F1 12 Tf 72 720 Td (The lamp is dark until it is switched on.) Tj ET\n\
                 q 80 0 0 80 100 600 cm /Im1 Do Q\n";
    let samples: [u8; 12] = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
    let bodies: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
          /Resources << /Font << /F1 4 0 R >> /XObject << /Im1 5 0 R >> >> /Contents 6 0 R >>"
            .to_vec(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
        [
            b"<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceRGB \
              /BitsPerComponent 8 /Length 12 >>\nstream\n"
                .to_vec(),
            samples.to_vec(),
            b"\nendstream".to_vec(),
        ]
        .concat(),
        [
            format!("<< /Length {} >>\nstream\n", text.len()).into_bytes(),
            text.to_vec(),
            b"endstream".to_vec(),
        ]
        .concat(),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, body) in bodies.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", index + 1).into_bytes());
        out.extend(body);
        out.extend(b"\nendobj\n");
    }
    let table = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", bodies.len() + 1).into_bytes());
    for offset in offsets {
        out.extend(format!("{offset:010} 00000 n \n").into_bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{table}\n%%EOF\n",
            bodies.len() + 1
        )
        .into_bytes(),
    );
    let path = dir.join("lamp.pdf");
    std::fs::write(&path, out).expect("the PDF was not written");
    path
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

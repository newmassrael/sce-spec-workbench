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
    let samples: [u8; 12] = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
    pdf_of_one_image(
        dir,
        "/Width 2 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8",
        &samples,
        (80, 80),
    )
}

/// [`lamp_pdf`] holding the lamp states drawing, as the JPEG the document would store it: the picture a
/// model is asked to read in a PDF, whose words every reading has to carry.
pub fn lamp_states_pdf(dir: &Path) -> PathBuf {
    let jpeg =
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lamp-states.jpg"))
            .expect("the JPEG of the drawing is in fixtures/");
    pdf_of_one_image(
        dir,
        "/Width 560 /Height 240 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode",
        &jpeg,
        (420, 180),
    )
}

/// A PDF of one page that says a sentence and draws one image under it, `placed` points wide and high.
/// `image_entries` are the image dictionary's entries that say how `image` is to be read.
fn pdf_of_one_image(dir: &Path, image_entries: &str, image: &[u8], placed: (u32, u32)) -> PathBuf {
    let text = format!(
        "BT /F1 12 Tf 72 720 Td (The lamp is dark until it is switched on.) Tj ET\n\
         q {} 0 0 {} 100 500 cm /Im1 Do Q\n",
        placed.0, placed.1
    )
    .into_bytes();
    let bodies: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
          /Resources << /Font << /F1 4 0 R >> /XObject << /Im1 5 0 R >> >> /Contents 6 0 R >>"
            .to_vec(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
        [
            format!(
                "<< /Type /XObject /Subtype /Image {image_entries} /Length {} >>\nstream\n",
                image.len()
            )
            .into_bytes(),
            image.to_vec(),
            b"\nendstream".to_vec(),
        ]
        .concat(),
        [
            format!("<< /Length {} >>\nstream\n", text.len()).into_bytes(),
            text,
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

/// The words the lamp states picture shows, each of which a reading that copies what is legible carries.
pub const DRAWN: [&str; 6] = [
    "dark",
    "lit",
    "fault",
    "switch.on",
    "after 30s",
    "bulb.broken",
];

/// A Markdown specification in `work` that marks the lamp states picture beside it: the document a live
/// test of a reader reads the picture of.
pub fn lamp_markdown(work: &Path) -> &'static str {
    std::fs::write(
        work.join("lamp.md"),
        "# Lamp\n\nThe lamp is dark until it is switched on.\n\n[picture: lamp-states.png]\n\nFigure 2. Lamp states\n",
    )
    .unwrap();
    std::fs::copy(lamp_states_png(), work.join("lamp-states.png")).unwrap();
    "lamp.md"
}

/// Run the authoring tools in `dir` and give back what they printed; a failure is the test's.
fn run_tool(dir: &Path, args: &[&str]) -> String {
    let (code, out, err) = tool(dir, args);
    assert_eq!(code, 0, "{args:?} failed:\n{out}\n{err}");
    out
}

/// The whole path of a live test of a reader: `document` in `work` is extracted, its first figure is
/// prepared for reading, `ask` (the real client) reads the picture, the tools add what it read, and the
/// check asks about it. Every word of the drawing has to be in what was read.
pub fn read_the_first_figure_of(
    work: &Path,
    document: &str,
    runtime: &str,
    ask: &dyn Fn(
        &sce_spec_workbench_core::claude_code::PictureQuestion<'_>,
    ) -> sce_spec_workbench_core::claude_code::PictureAnswer,
) {
    let work = work.to_path_buf();
    run_tool(&work, &["extract", document, "--out", "x.json"]);
    run_tool(
        &work,
        &["convert", "--extraction", "x.json", "--out", "c.json"],
    );
    let extraction: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(work.join("x.json")).unwrap()).unwrap();
    let figure = extraction["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|block| block["kind"] == "figure")
        .and_then(|block| block["id"].as_str())
        .expect("the mark is read as a figure")
        .to_string();
    run_tool(
        &work,
        &[
            "prepare-reading",
            "--extraction",
            "x.json",
            "--document",
            document,
            "--block",
            &figure,
            "--out-dir",
            "units",
        ],
    );
    let units = work.join("units");
    let listing: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(units.join("units.json")).unwrap()).unwrap();
    let unit = &listing["units"][0];
    let file = |field: &str| units.join(unit[field].as_str().unwrap());
    let prompt = std::fs::read_to_string(file("prompt")).unwrap();
    let picture = std::fs::read(file("file")).unwrap();
    let schema: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(units.join("answer.schema.json")).unwrap())
            .unwrap();

    let read = ask(&sce_spec_workbench_core::claude_code::PictureQuestion {
        prompt: &prompt,
        picture: &picture,
        media_type: unit["picture"]["media_type"].as_str().unwrap(),
        answer_schema: &schema,
    });
    std::fs::write(file("answer"), read.answer.to_string()).unwrap();
    let model = read.model.clone().unwrap_or_default();
    println!("model: {model}");

    let finished = run_tool(
        &work,
        &[
            "finish-reading",
            "--extraction",
            "x.json",
            "--conversion",
            "c.json",
            "--dir",
            "units",
            "--runtime",
            runtime,
            "--model",
            &model,
            "--out",
            "c2.json",
            "--json",
        ],
    );
    println!("finish-reading: {finished}");
    let checked = run_tool(
        &work,
        &[
            "check-conversion",
            "--extraction",
            "x.json",
            "--conversion",
            "c2.json",
            "--json",
        ],
    );
    let report: serde_json::Value = serde_json::from_str(&checked).unwrap();
    let asked = report["read_from_picture"].as_array().unwrap();
    assert!(!asked.is_empty(), "nothing read was asked about: {report}");

    let conversion: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(work.join("c2.json")).unwrap()).unwrap();
    let read_text: String = conversion["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|block| block.get("picture").is_some())
        .map(|block| block["markdown"].as_str().unwrap().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    for word in DRAWN {
        assert!(
            read_text.contains(word),
            "`{word}` is not in what was read:\n{read_text}"
        );
    }
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

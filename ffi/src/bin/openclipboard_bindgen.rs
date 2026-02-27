use anyhow::{Context, Result};
use camino::Utf8PathBuf;

fn main() -> Result<()> {
    // Minimal arg parsing (kept dependency-free).
    // Usage:
    //   cargo run -p openclipboard_ffi --bin openclipboard-bindgen -- \
    //     --language kotlin|swift --udl <path> --out <dir> [--library <cdylib>]

    let mut language: Option<String> = None;
    let mut udl: Option<Utf8PathBuf> = None;
    let mut out: Option<Utf8PathBuf> = None;
    let mut library: Option<Utf8PathBuf> = None;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--language" => language = it.next(),
            "--udl" => udl = it.next().map(Utf8PathBuf::from),
            "--out" => out = it.next().map(Utf8PathBuf::from),
            "--library" => library = it.next().map(Utf8PathBuf::from),
            "-h" | "--help" => {
                eprintln!(
                    "openclipboard-bindgen\n\n  --language kotlin|swift\n  --udl <path>\n  --out <dir>\n  [--library <cdylib-path>]\n"
                );
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown arg: {other}"),
        }
    }

    let language = language.context("--language is required")?;
    let udl = udl.context("--udl is required")?;
    let out = out.context("--out is required")?;

    match language.as_str() {
        "kotlin" => {
            let generator = uniffi_bindgen::bindings::KotlinBindingGenerator;
            uniffi_bindgen::generate_external_bindings(
                &generator,
                &udl,
                Option::<&camino::Utf8PathBuf>::None,
                Some(&out),
                library.as_ref(),
                None,
                false,
            )?;
        }
        "swift" => {
            let generator = uniffi_bindgen::bindings::SwiftBindingGenerator;
            uniffi_bindgen::generate_external_bindings(
                &generator,
                &udl,
                Option::<&camino::Utf8PathBuf>::None,
                Some(&out),
                library.as_ref(),
                None,
                false,
            )?;
        }
        _ => anyhow::bail!("unsupported --language {language} (expected kotlin|swift)"),
    }

    Ok(())
}

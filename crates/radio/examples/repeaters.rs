//! Convert RepeaterBook KML exports into the repeaters file the app reads.
//! `repeaters [--out FILE] <file.kml>...`
//! Pass the state's "All" file plus any per-mode files ("Colorado DMR.kml") to tag modes.
//! The app keeps those near the location set in its controls.

use radio::repeaters::{file_mode, merge, parse_kml, Repeaters};

fn main() -> anyhow::Result<()> {
    let usage = "usage: repeaters [--out FILE] <file.kml>...";
    let (mut out, mut files) = (Repeaters::path(), vec![]);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--out" => out = Some(args.next().expect(usage).into()),
            _ => files.push(a),
        }
    }
    assert!(!files.is_empty(), "{usage}");
    let mut parsed = vec![];
    for f in &files {
        let list = parse_kml(&std::fs::read_to_string(f)?);
        let name = std::path::Path::new(f).file_name().unwrap_or_default().to_string_lossy();
        println!("{f}: {} repeaters{}", list.len(), file_mode(&name).map(|m| format!(", tagged {m}")).unwrap_or_default());
        parsed.push((file_mode(&name), list));
    }
    let list = merge(parsed);
    let out = out.expect("no config directory; pass --out");
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let header = "# Repeaters from RepeaterBook KML exports, written by `cargo run -p radio --example repeaters`.\n\
                  # Rerun it to refresh; hand edits are kept only until then.\n\n";
    std::fs::write(&out, header.to_string() + &toml::to_string(&Repeaters { list: list.clone() })?)?;
    println!("wrote {} on-air repeaters to {}", list.len(), out.display());
    Ok(())
}

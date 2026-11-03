//! Convert RepeaterBook KML exports into the repeaters file the app reads.
//! `repeaters [--out FILE] <file.kml>...`
//! Pass the US-wide `doc.kml` (unzip the .kmz), or a state's "All" file plus any per-mode files
//! ("Colorado DMR.kml") to tag modes.
//! The app keeps those near the location set in its controls.

use radio::repeaters::{merge, parse_export, Repeaters};

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
        let name = std::path::Path::new(f).file_name().unwrap_or_default().to_string_lossy();
        let lists = parse_export(&name, &std::fs::read_to_string(f)?);
        println!("{f}: {} lists, {} entries", lists.len(), lists.iter().map(|(_, l)| l.len()).sum::<usize>());
        parsed.extend(lists);
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

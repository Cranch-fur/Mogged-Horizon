// A native mod's settings page: its buttons whose label starts with U+0001
// become lines of grey text (`Row::Note`, as under the mod's name), for
// Mogged's skin paths. The button stays in the list, so the others keep
// their indexes; the text line just can't be clicked.
fn mogged_notes(value: Vec<Row>, s: &Settings, i: usize) -> Vec<Row> {
    let mut rows = value;
    let Some(m) = s.mods.installed.get(i) else {
        return rows;
    };
    let Some(page) = &m.native else {
        return rows;
    };
    // The mod's name, then a row per key, then a row per button.
    let first = 1 + page.keys.len();
    for (b, label) in page.buttons.iter().enumerate() {
        if let Some(note) = label.strip_prefix('\u{1}') {
            if let Some(row) = rows.get_mut(first + b) {
                *row = Row::Note(note.to_owned());
            }
        }
    }
    rows
}

// Mogged's row in Options > Mods, with its key.
fn mogged_listed(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    crate::game::mogged_glue::listed(mods);
}

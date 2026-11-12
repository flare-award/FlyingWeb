fn fw_mods(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    crate::game::flyingweb_glue::on_mods_list(mods);
}

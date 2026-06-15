use std::{env, fs, io, path::Path};

const CONFIG_ENV_VARS: &[&str] = &[
    "QUIRE_APP_ID",
    "QUIRE_GETTEXT_PACKAGE",
    "QUIRE_LOCALEDIR",
    "QUIRE_PKGDATADIR",
    "QUIRE_PROFILE",
    "QUIRE_VERSION",
];

fn main() -> io::Result<()> {
    for name in CONFIG_ENV_VARS {
        println!("cargo:rerun-if-env-changed={name}");
    }

    let app_id = env_or("QUIRE_APP_ID", "com.fvtronics.quire");
    let gettext_package = env_or("QUIRE_GETTEXT_PACKAGE", "quire");
    let localedir = env_or("QUIRE_LOCALEDIR", "/usr/local/share/locale");
    let pkgdatadir = env_or("QUIRE_PKGDATADIR", "/usr/local/share/quire");
    let profile = env_or("QUIRE_PROFILE", "");
    let version =
        env::var("QUIRE_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_owned());

    let config = format!(
        "pub const APP_ID: &str = {app_id:?};\n\
         pub const GETTEXT_PACKAGE: &str = {gettext_package:?};\n\
         pub const LOCALEDIR: &str = {localedir:?};\n\
         #[allow(unused)]\n\
         pub const PKGDATADIR: &str = {pkgdatadir:?};\n\
         pub const PROFILE: &str = {profile:?};\n\
         pub const RESOURCES_FILE: &str = concat!({pkgdatadir:?}, \"/resources.gresource\");\n\
         pub const VERSION: &str = {version:?};\n",
    );

    let out_dir = env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo");
    fs::write(Path::new(&out_dir).join("config.rs"), config)
}

fn env_or(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_owned())
}

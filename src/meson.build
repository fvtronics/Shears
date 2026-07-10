cargo_options = [ '--manifest-path', meson.project_source_root() / 'Cargo.toml' ]
cargo_options += [ '--target-dir', meson.project_build_root() / 'src' ]

if get_option('profile') == 'default'
  cargo_options += [ '--release' ]
  rust_target = 'release'
  message('Building in release mode')
else
  rust_target = 'debug'
  message('Building in debug mode')
endif

cargo_home = get_option('cargo-home')

if cargo_home == ''
  cargo_home = meson.project_build_root() / 'cargo-home'
endif

cargo_env = [
  'CARGO_HOME=' + cargo_home,
  'SHEARS_APP_ID=' + application_id,
  'SHEARS_GETTEXT_PACKAGE=' + gettext_package,
  'SHEARS_LOCALEDIR=' + localedir,
  'SHEARS_PKGDATADIR=' + pkgdatadir,
  'SHEARS_PROFILE=' + profile,
  'SHEARS_VERSION=' + version + version_suffix,
]

cargo_build = custom_target(
  'cargo-build',
  build_by_default: true,
  build_always_stale: true,
  output: meson.project_name(),
  console: true,
  install: true,
  install_dir: bindir,
  depends: resources,
  command: [
    'env',
    cargo_env,
    cargo, 'build',
    cargo_options,
    '&&',
    'cp', 'src' / rust_target / meson.project_name(), '@OUTPUT@',
  ]
)

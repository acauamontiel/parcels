blueprints = custom_target(
  'blueprints',
  input: files(
    'ui/shortcuts-dialog.blp',
    'ui/window.blp',
  ),
  output: '.',
  command: [
    blueprint_compiler,
    'batch-compile',
    '@OUTPUT@',
    '@CURRENT_SOURCE_DIR@',
    '@INPUT@',
  ],
)

resources = gnome.compile_resources(
  'resources',
  'resources.gresource.xml',
  gresource_bundle: true,
  source_dir: meson.current_build_dir(),
  dependencies: blueprints,
  install: true,
  install_dir: pkgdatadir,
)

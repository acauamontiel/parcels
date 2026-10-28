blueprints = custom_target(
  'blueprints',
  input: files(
    'ui/add-dialog.blp',
    'ui/server-dialog.blp',
    'ui/shortcuts-dialog.blp',
    'ui/torrent-page.blp',
    'ui/torrent-row.blp',
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

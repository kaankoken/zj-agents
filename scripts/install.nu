#!/usr/bin/env nu
# Install zj-agents WASM plugins into Zellij's plugin directory.
# Nushell-native (no bash brace expansion).

def main [
  --release (-r)  # build release wasm before install (default: true if artifacts missing)
] {
  let repo = ($env.FILE_PWD | path dirname)
  let out = ($repo | path join "target" "wasm32-wasip1" "release")
  let engine = ($out | path join "zj-agents-engine.wasm")
  let sidebar = ($out | path join "zj-agents-sidebar.wasm")
  let dest = ($env.HOME | path join ".config" "zellij" "plugins")

  let need_build = (not ($engine | path exists)) or (not ($sidebar | path exists)) or $release
  if $need_build {
    print "Building release WASM (wasm32-wasip1)…"
    cd $repo
    ^cargo build --workspace --release --target wasm32-wasip1
  }

  if not ($engine | path exists) or not ($sidebar | path exists) {
    error make {msg: $"Missing WASM artifacts under ($out)"}
  }

  mkdir $dest
  cp $engine $sidebar $dest
  print $"Installed:\n  ($dest | path join 'zj-agents-engine.wasm')\n  ($dest | path join 'zj-agents-sidebar.wasm')"
  print "Register aliases in Zellij config (plugins { zj-agents-engine … }) then start a new session."
}

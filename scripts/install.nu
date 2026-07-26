#!/usr/bin/env nu
# Install zj-agents WASM plugins into Zellij's plugin directory.
# Nushell-native (no bash brace expansion).

def main [
  --from-release (-r)  # download latest GitHub release assets instead of building
  --tag: string = "latest"  # release tag when using --from-release (default: latest)
  --repo: string = "kaankoken/zj-agents"
  --build  # force local cargo release build even if artifacts exist
] {
  let dest = ($env.HOME | path join ".config" "zellij" "plugins")
  mkdir $dest

  if $from_release {
    install_from_release $repo $tag $dest
  } else {
    install_from_source $build $dest
  }

  print $"Installed:\n  ($dest | path join 'zj-agents-engine.wasm')\n  ($dest | path join 'zj-agents-sidebar.wasm')"
  print "Add named plugin aliases in your Zellij config (see README)."
}

def install_from_source [force_build: bool, dest: path] {
  let repo = ($env.FILE_PWD | path dirname)
  let out = ($repo | path join "target" "wasm32-wasip1" "release")
  let engine = ($out | path join "zj-agents-engine.wasm")
  let sidebar = ($out | path join "zj-agents-sidebar.wasm")

  let missing = (not ($engine | path exists)) or (not ($sidebar | path exists))
  if $force_build or $missing {
    print "Building release WASM (wasm32-wasip1)…"
    cd $repo
    ^cargo build --workspace --release --target wasm32-wasip1
  }

  if not ($engine | path exists) or not ($sidebar | path exists) {
    error make {msg: $"Missing WASM artifacts under ($out)"}
  }

  cp $engine $sidebar $dest
}

def install_from_release [repo: string, tag: string, dest: path] {
  let base = if $tag == "latest" {
    $"https://github.com/($repo)/releases/latest/download"
  } else {
    $"https://github.com/($repo)/releases/download/($tag)"
  }

  for name in ["zj-agents-engine.wasm" "zj-agents-sidebar.wasm"] {
    let url = $"($base)/($name)"
    let path = ($dest | path join $name)
    print $"Downloading ($url)…"
    http get $url | save --force $path
  }
}

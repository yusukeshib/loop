# loop

A tiny, portable, Kubernetes-shaped control loop for your work.

`loop` is a single self-contained bash script. Install it by putting it on your
`PATH` — pick whichever method you like below.

## Install

### curl (recommended)

```sh
curl -fsSL https://raw.githubusercontent.com/yusukeshib/loop/main/install.sh | bash
```

This downloads the `loop` script to `~/.local/bin/loop` and makes it executable.
Override the destination with `LOOP_INSTALL_DIR`, or pin a ref with `LOOP_REF`:

```sh
LOOP_INSTALL_DIR=/usr/local/bin LOOP_REF=v0.1.0 \
  curl -fsSL https://raw.githubusercontent.com/yusukeshib/loop/main/install.sh | bash
```

Make sure your install dir is on your `PATH` (the installer warns you if not):

```sh
export PATH="$HOME/.local/bin:$PATH"
```

### Nix (flakes)

Run it directly without installing:

```sh
nix run github:yusukeshib/loop
```

Install into your profile:

```sh
nix profile install github:yusukeshib/loop
```

Or add it to a flake as an input and use `inputs.loop.packages.<system>.default`.
A dev shell with the runtime deps (`bash`, `git`, `jq`, …) is available via
`nix develop github:yusukeshib/loop`.

### Manual

Clone the repo and symlink the script onto your `PATH`:

```sh
git clone https://github.com/yusukeshib/loop.git
ln -s "$PWD/loop/loop" ~/.local/bin/loop
```

## Verify

```sh
loop version   # -> loop 0.1.0
loop help
```

## Data & config

State/memory lives separately in `git@github.com:yusukeshib/loop_state.git`
(cloned to `$XDG_STATE_HOME/loop`). Runner config: `$XDG_CONFIG_HOME/loop.json`.

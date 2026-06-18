# looop

A tiny, portable, Kubernetes-shaped control loop for your work.

`looop` is a single self-contained bash script. Install it by putting it on your
`PATH` — pick whichever method you like below.

## Install

### curl (recommended)

```sh
curl -fsSL https://raw.githubusercontent.com/yusukeshib/looop/main/install.sh | bash
```

This downloads the `looop` script to `~/.local/bin/looop` and makes it executable.
Override the destination with `LOOOP_INSTALL_DIR`, or pin a ref with `LOOOP_REF`:

```sh
LOOOP_INSTALL_DIR=/usr/local/bin LOOOP_REF=v0.2.0 \
  curl -fsSL https://raw.githubusercontent.com/yusukeshib/looop/main/install.sh | bash
```

Make sure your install dir is on your `PATH` (the installer warns you if not):

```sh
export PATH="$HOME/.local/bin:$PATH"
```

### Nix (flakes)

Run it directly without installing:

```sh
nix run github:yusukeshib/looop
```

Install into your profile:

```sh
nix profile install github:yusukeshib/looop
```

Or add it to a flake as an input and use `inputs.looop.packages.<system>.default`.
A dev shell with the runtime deps (`bash`, `git`, `jq`, …) is available via
`nix develop github:yusukeshib/looop`.

### Manual

Clone the repo and symlink the script onto your `PATH`:

```sh
git clone https://github.com/yusukeshib/looop.git
ln -s "$PWD/looop/looop" ~/.local/bin/looop
```

## Verify

```sh
looop version   # -> looop 0.8.0
looop help
```

## Data & config

State/memory lives separately in `git@github.com:yusukeshib/looop_state.git`
(cloned to `$XDG_STATE_HOME/looop`). Runner config: `$XDG_CONFIG_HOME/looop.json`.

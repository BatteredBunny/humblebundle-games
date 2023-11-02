# humblebundle-games

Simple CLI tool to display all unclaimed keys you have on your humble bundle account.

![screenshot](screenshot.png)

```
Humble bundle keys

Usage: humblebundle-games [OPTIONS] --token <TOKEN>

Options:
  -t, --token <TOKEN>  _simpleauth_sess cookie value
  -j, --json           return data in json
  -h, --help           Print help
  -V, --version        Print version
```

## Usage

Find ``_simpleauth_sess`` cookie from browser and use it in token flag
```
git clone https://github.com/ayes-web/humblebundle-games
cd humblebundle-games
cargo run --release -- --token {YOUR COOKIE}
```


## Run with nix flake
```
nix run github:ayes-web/humblebundle-games -- --token {YOUR COOKIE}
```
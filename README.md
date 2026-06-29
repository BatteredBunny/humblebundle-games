# humblebundle-games

Simple CLI tool to display all unclaimed keys you have on your humble bundle account.

![normal text output](screenshot.png)
![csv output opened in macos numbers](screenshot_csv.png)

```
Humble bundle keys

Usage: humblebundle-games [OPTIONS] --token <TOKEN>

Options:
  -t, --token <TOKEN>    _simpleauth_sess cookie value
  -f, --format <FORMAT>  format to output data in [default: text] [possible values: json, csv, text]
  -h, --help             Print help
  -V, --version          Print version
```

## Usage

Find ``_simpleauth_sess`` cookie from browser and use it in token flag
```
git clone https://github.com/BatteredBunny/humblebundle-games
cd humblebundle-games
cargo run --release -- --token {YOUR COOKIE}
```


## Run with nix flake
```
nix run github:BatteredBunny/humblebundle-games -- --token {YOUR COOKIE}
```


## Output example

``
cargo run --release -- --format json
``

```json
[
    {
        "key": "Boomerang Fu",
        "choice_url": "https://www.humblebundle.com/membership/february-2021",
        "platform": "steam"
    },
    {
        "key": "Werewolf: The Apocalypse — Heart of the Forest",
        "choice_url": "https://www.humblebundle.com/membership/february-2021",
        "platform": "steam"
    },
    {
        "key": "Trine 4: The Nightmare Prince",
        "choice_url": "https://www.humblebundle.com/membership/february-2021",
        "platform": "steam"
    },
]
```

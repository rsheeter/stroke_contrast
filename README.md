# Exploratory hackery

Quick and dirty exploration of finding stroke width.

## Run one

```shell
$ cargo run -- -c o --font ~/oss/fonts/ofl/lobster/Lobster-Regular.ttf --method all-segments
$ cargo run -- -c o --font ~/oss/fonts/ofl/lobster/Lobster-Regular.ttf --method center-of-mass

$ cargo run -- -c o --font ~/oss/fonts/ofl/ballet/Ballet[opsz].ttf --method all-segments
$ cargo run -- -c o --font ~/oss/fonts/ofl/allura/Allura-Regular.ttf --method all-segments
$ cargo run -- -c o --font ~/oss/fonts/ofl/changaone/ChangaOne-Regular.ttf --method all-segments
$ cargo run -- -c o --font ~/oss/fonts/ofl/lilitaone/LilitaOne-Regular.ttf --method all-segments


$ cargo run -- -c o --font ~/oss/fonts/ofl/rubikglitch/RubikGlitch-Regular.ttf --method all-segments
	- hangs
$ cargo run -- -c o --font ~/oss/fonts/ofl/rubikglitch/RubikGlitch-Regular.ttf --method center-of-mass
	- poor result

$ cargo run -- -c o --font ~/oss/fonts/ofl/allura/Allura-Regular.ttf --method center-of-mass

# Fun because it has holes
$ cargo run -- -c o --font ~/oss/fonts/ofl/kablammo/Kablammo[MORF].ttf --method center-of-mass
$ cargo run -- -c o --font ~/oss/fonts/ofl/kablammo/Kablammo[MORF].ttf --method all-segments
```

## Run batch

```shell
$ cargo build --release

# Targeting is used because some families (think Rubik Glitch) don't get good results
# By default only families that don't yet have values are processed
$ target/release/batch --tag-filter "/Expressive/Business"
```
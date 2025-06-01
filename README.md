
Ill-formed idea:

* Fail if center of mass is inside shape
* Cast rays from center of mass
* Find intersections of path segments and rays
* For each ray, find the nearest intersection
    * this _should_ be a transition from not-ink (nearer center of mass) to ink


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
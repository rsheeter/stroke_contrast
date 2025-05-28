
Ill-formed idea:

* Fail if center of mass is inside shape
* Cast rays from center of mass
* Find intersections of path segments and rays
* For each ray, find the nearest intersection
    * this _should_ be a transition from not-ink (nearer center of mass) to ink


```shell
$ $ cargo run -- -c o --font ~/oss/fonts/ofl/allura/Allura-Regular.ttf
```
# Fmc 
![picture](github/picture.png)
# How to run
Rust nightly **required** and [mold linker](https://github.com/rui314/mold) if you use linux.
```
cd server && cargo run --release
cd client && cargo run --release
```
# Controls
| key              |                                                                               |
| ---------        | -----------                                                                   |
| `Escape`           | Quit                                                                          |
| `WASD`             | Move                                                                          |
| `Spacebar`         | Jump/Fly, double tap to toggle flight                                         |
| `Shift`            | Down when flying                                                              |
| `Control`          | Speed up horizontal flight                                                    |
| `Left click`       | Mine block                                                                    |
| `Right click`      | Place block                                                                   |
| `e`                | inventory                                                                     |
| `Shift+left click` | Halve inventory stack / place one item, yes I will fix this to be right click |

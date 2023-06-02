# Fmc 
![picture](github/picture.png)
# How to run
Rust nightly **required** and [mold linker](https://github.com/rui314/mold) if you use linux.
```
cd server && cargo run --release
cd client && cargo run --release
```
# Controls
| key                |                                                                             |
| ---------          | -----------                                                                 |
| `Escape`           | Quit                                                                        |
| `WASD`             | Move                                                                        |
| `Spacebar`         | Jump/Fly, double tap to toggle flight                                       |
| `Shift`            | Down when flying                                                            |
| `Control`          | Speed up horizontal flight                                                  |
| `Left click`       | Mine block                                                                  |
| `Right click`      | Place block                                                                 |
| `e`                | inventory                                                                   |
| `Shift+left click` | Halve inventory stack / place one item  (I will fix this to be right click) |

# Copyright/Licensing
Copyright 2023 Awowogei

Fmc is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License version 3 as published by the Free Software Foundation.

Fmc is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License along with Fmc. If not, see <https://www.gnu.org/licenses/>. 


Linking Fmc statically or dynamically with other modules is making a combined work based on Fmc. Thus, the terms and conditions of the GNU Affero Public License cover the whole combination.

As a special exception, the copyright holders of Fmc give you permission to combine the Fmc client/server with independent modules that communicate with the Fmc client/server solely through the Fmc client/server API. You may copy and distribute such a system following the terms of the GNU AGPL for Fmc and the licenses of the other code concerned, provided that you include the source code of that other code when and as the GNU AGPL requires distribution of source code and provided that you do not modify the Fmc client/server modding interface.

If you modify the Fmc client/server API, this exception does not apply to your modified version of Fmc, and you must remove this exception when you distribute your modified version.

This exception is an additional permission under section 7 of the GNU Affero Public License, version 3 (“AGPLv3”)

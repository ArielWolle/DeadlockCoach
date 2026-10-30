# Minimap regression fixtures

Inputs for `crates/vision/tests/minimap.rs` and `crates/vision/tests/reliability.rs`.

| File | Fixture purpose |
| --- | --- |
| `no-urn.png`, `pre-urn.png`, `urn-*.png` | Minimap-only crops (calibration recording): urn states; `no-urn` is also the map locator template |
| `rec5-122-ping.jpg` | Red ping circle that read as six enemy rings |
| `rec5-150-urn-carry.jpg` | You carrying the urn: marker replaced by a small vase under your view cone |
| `rec5-250-wave.jpg` | Enemy trooper wave merged into one blob on the Green lane (mirrored layout) |
| `rec5-480-york-factory.jpg` | Zone label `YORK : FACTORY`, you on the left (Yellow) lane, mirrored layout |
| `rec5-560-hidden-king.jpg` | Zone label `HIDDEN KING : BASE` (own base) and lit scenery past the map rim |
| `rec13-040-structures-standing.png` | Lane structures (`crates/vision/tests/gamestate.rs`): all 24 diamonds standing at ~4:40 |
| `rec13-520-structures-late.png` | Lane structures at 36:40: every enemy lane structure fallen (their Shrines and Patron standing); yours down to the right Base Guardian, Shrines and Patron |
| `topbar-rec5-210.jpg` | Top bar: own portrait backdrop in ally slot 0, Haze (dead, greyed) in slot 3 |
| `topbar-rec4-134.jpg` | Top bar: own portrait backdrop in ally slot 4, Haze (Sapphire) in slot 0 |

`rec5-*` crops are the 1920x1080 frame region x 1416-1920, y 600-1080 (minimap plus its zone
label); `rec13-*-structures-*` are the minimap crop alone (x 1506-1908, y 630-1050, the
`minimap::crop_from_frame` box); `topbar-*` are the top 84 rows of a 1920x1080 frame. Tests paste them back into a
black 1920x1080 frame.

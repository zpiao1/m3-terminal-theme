# Icon

`icon.svg` (256px, ships as the large icon) and `icon-small.svg` (the 16-32px cut,
simplified so it stays legible) are the sources. `icon.ico` is generated from them and
embedded into the exe by `build.rs` via `app.rc`. It holds a separate bitmap at each
size, so Windows never has to downscale.

Regenerate after editing an SVG (the `resvg` CLI is `cargo install resvg`; any ICO packer
that keeps per-size bitmaps works, shown with Pillow):

```sh
for sz in 256 64 48 40 32 24 20 16; do
  src=icon.svg; [ $sz -le 32 ] && src=icon-small.svg
  resvg -w $sz -h $sz $src $sz.png
done
python -c "from PIL import Image; ims=[Image.open(f'{s}.png') for s in (256,64,48,40,32,24,20,16)]; ims[0].save('icon.ico', sizes=[i.size for i in ims], append_images=ims[1:])"
rm ./*.png
```

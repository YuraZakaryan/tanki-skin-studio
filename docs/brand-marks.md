# Brand marks

About links to the project's Telegram channel and chat and to its YouTube video guides. Each link
shows its service's own mark, taken from the owner's official download and used as the owner's
guidelines allow. The marks are trademarks of their owners, not under this project's licence
(`LICENSES/LicenseRef-Trademark.txt`). About says, under the links: "Telegram and YouTube are
trademarks of their respective owners. Tanki Skin Studio is not affiliated with or endorsed by
them."

| File | Source | How it was made |
| --- | --- | --- |
| `crates/tanki-ui/assets/brand/telegram.png` | `Logo.svg` from Telegram's logo archive on [telegram.org/tour/screenshots](https://telegram.org/tour/screenshots) (archive sha256 `8b1924b30790b3557cd1a76c4028403035dae386f282aaa3d741c5bc5a03d8a7`, `Logo.svg` sha256 `54842d414c100f0110f90caf09ff58cd0d229629820e4576bd70befca88befd5`) | `resvg -w 112 -h 112 Logo.svg telegram.png` |
| `crates/tanki-ui/assets/brand/youtube.png` | `YouTube_Icon/Digital/01 Red/yt_icon_red_digital.png` from the icon archive on [brand.youtube/youtube-icon](https://brand.youtube/youtube-icon/) (archive sha256 `ca9b5104387e0f7afcfda3a79c910449561112f1077177dd0d64f8c72f56e476`, PNG sha256 `1027b1b0517727adb9697155a270744381c3ce9b047b1c8bd8a9389dc7d07a83`) | `magick yt_icon_red_digital.png -filter Lanczos -resize 168x144 PNG32:youtube.png` |

## The rules they follow

- **Telegram** lets anyone use its logos for "forward to Telegram" buttons and the like, as long as
  it is clear the user is not Telegram.
- **YouTube** asks for its icon, not its logo, when a channel is linked among other social icons.
  The icon stays unchanged (no recolouring, cropping, stretching, rotating or shadow) and keeps its
  clear space; the official PNG already carries that space, so it is only scaled down. Other uses
  go through YouTube's brand approval form.

Both are drawn at their own proportions, in place of a row's tinted icon box: Telegram's logo
28 points across, YouTube's icon 36 points wide (`tanki_ui::brand`). The same files are used by
`design.pen`. To refresh one, download the owner's current archive, make the file the same way,
update the hashes here and check the result in About in both themes.

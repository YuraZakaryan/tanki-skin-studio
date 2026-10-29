# Skin pack v1

A pack is a `.tankipack` ZIP or a folder containing `manifest.json`. A cloned Git repository folder can be imported directly. The manifest names semantic targets; it never names a destination path in the user's cache. The importer verifies every declared file with SHA-256 and enforces file/archive size and path limits.

```json
{
  "format_version": 1,
  "id": "example.classic-combo",
  "name": "Classic Combo",
  "version": "1.0.0",
  "author": "Example Author",
  "license": "CC-BY-4.0",
  "games": ["air-cache"],
  "items": [
    {
      "target": { "kind": "hull", "id": "wasp", "grade": "m3" },
      "variants": [
        {
          "id": "legacy",
          "name": "Legacy",
          "preview": "previews/wasp.png",
          "resources": [
            { "role": "model", "path": "assets/wasp/object.3ds", "sha256": "<64 hex digits>" },
            { "role": "details", "path": "assets/wasp/details.png", "sha256": "<64 hex digits>" },
            { "role": "lightmap", "path": "assets/wasp/lightmap.jpg", "sha256": "<64 hex digits>" }
          ]
        }
      ]
    }
  ]
}
```

The placeholder hashes above must be replaced with actual SHA-256 digests. For a Railgun variant, use `{"kind":"turret","id":"railgun","grade":"m3"}`. Supported apply roles today are `model`, `details`, `details_alpha`, and `lightmap`. PNG `details` is converted into JPEG color and alpha resources. If `details` is a JPEG, provide `details_alpha` separately when alpha must change. Preview can be PNG or JPEG. Other role names can be stored in a pack but cannot yet be applied.

A target category describes the game item (`hull`, `turret`, later `paint`, `map`, `interface`, `currency`, `effect`, `sound`). A skin is a variant of that item. The adapter decides which local cache files represent it. Items are recognised by their content (see the README), and `games` names the cache format, `air-cache`, never a game.

## Folders without a manifest

A folder of models and textures with no `manifest.json` (a community archive, a cloned repository, a
folder of skins) is turned into a pack on import. No layout is assumed; the whole tree is read to any
depth, and every folder holding a model or a picture is read from its path, word by word:

- a kind word says hull or turret: `hulls`, `hull`, `tanks`, `tank`, `turrets`, `weapons`, `guns`;
- a known item name says the item, also spelt otherwise (`flamethrower`) or one typo away (`mamoth`,
  `frezee`); below a kind folder an unknown name is taken as a new item and reported;
- `old`, `new`, `legacy`, `classic` are the style of a variant;
- what is said at and below the item is the modification or edition: `M3`, `XT`, `XT M0` → `xt-m0`,
  `ALT/M2` → `alt-m2`, `Prime`;
- words between the kind folder and the item name the skin: `Hulls/Neon/Wasp/M3` is the `neon` variant;
- filing folders (`textures`, `model`, `files`...) say nothing, and folders of one skin are joined.

Examples that all work: `Flash/Hulls/New/Wasp/M3`, `2.0/hulls/wasp/m0`, `Weapons/Railgun/XT`, `tanks/wasp`,
`Smoky XT`. When the folder has real kind folders, a folder named only after an item elsewhere
(`Misc/Overdrives/Wasp`, an effect) is left out.

Files get their roles by name first, then by content: the model is `object.3ds` or the only 3DS file by its
signature; the lightmap a `lightmap*.jpg`; the texture `details.png` or a picture named `details`, `diffuse`,
`albedo`; its alpha `details_alpha.jpg`; the preview `preview.png`, a picture named `preview`, `icon` or `thumb`,
the only remaining picture, or the only one of the garage size (164 × 106). Anything ambiguous is left out.

The import report lists variants without a preview, files not used, formats the Flash client cannot use
(`a3d`, `ktx`, `webp`...), folders with no recognised item, folders left out, and guesses to check. The pack is
named after the folder and versioned by its content, so the same archive is not imported twice.

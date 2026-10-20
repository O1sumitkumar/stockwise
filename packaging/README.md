# Packaging

How StockWise gets from a release tag to package managers. Version numbers and
checksums are never edited by hand: `render.sh` fills them in from a release's
`.sha256` files.

| Channel | Source | How it is published |
|---------|--------|---------------------|
| Release archives | `.github/workflows/release.yml` | Automatic on every `v*` tag: binary, README, LICENSE, CHANGELOG, DISCLAIMER and `man/*.1` |
| Homebrew | `homebrew/stockwise.rb` | `homebrew` job pushes `Formula/stockwise.rb` to `iamsaquib8/homebrew-tap`, only when the `HOMEBREW_TAP_TOKEN` secret exists |
| Scoop | `scoop/stockwise.json` | `packaging` job attaches the rendered manifest to the release (`scoop install <release URL>`); `checkver`/`autoupdate` also suit a bucket |
| winget | `winget/*.yaml` | Manual pull request to `microsoft/winget-pkgs` |
| AUR | `aur/stockwise-bin/` | Manual push to `aur.archlinux.org/stockwise-bin.git` |
| Docker | `../Dockerfile` | `docker` + `docker-manifest` jobs push `ghcr.io/iamsaquib8/stockwise:<version>`, `:<major>.<minor>` and `:latest` (linux/amd64, linux/arm64) |
| Nix | `../flake.nix` | Builds from source; nothing to publish |

## Man pages

`stockwise man --dir man` writes `stockwise.1` and one `stockwise-<command>.1`
per command, rendered by `clap_mangen` from the CLI definition in `src/main.rs`.
The release workflow runs it once (`manpages` job) and puts the pages in every
archive under `man/`. Homebrew, the AUR package and the Nix flake install them.
There is no hand-written copy to keep in sync.

## Templates

Placeholders: `@VERSION@` and `@SHA256_<TARGET>@` (the Rust target in upper case
with `_` for `-`, e.g. `@SHA256_AARCH64_APPLE_DARWIN@`). Lines starting with
`#:` are template notes and are dropped from the output.

```bash
gh release download v0.3.0 -R iamsaquib8/stockwise --pattern '*.sha256' --dir sums
packaging/render.sh 0.3.0 sums dist
```

Every tagged release also uploads the rendered set as the `packaging-manifests`
workflow artifact, so the manual steps below can start from it.

## One-time setup (maintainer)

1. **Homebrew tap.** Create the public repository `iamsaquib8/homebrew-tap`.
   Create a fine-grained personal access token with *Contents: read and write*
   on that repository only, and save it as the Actions secret
   `HOMEBREW_TAP_TOKEN` in `iamsaquib8/stockwise`. The next tag pushes the
   formula. For a release that is already out, render it as above and commit
   `dist/homebrew/stockwise.rb` to `Formula/stockwise.rb` in the tap.
2. **Container visibility.** After the first tagged release, open the
   `stockwise` package under your GitHub profile → Packages → Package settings,
   and set its visibility to Public.
3. **winget.** Fork `microsoft/winget-pkgs`, copy
   `dist/winget/manifests/i/iamsaquib8/StockWise/<version>/` to the same path,
   check it with `winget validate --manifest <that dir>` on Windows, and open a
   pull request. Repeat per release (or automate it later with a winget
   releaser action and a token).
4. **AUR.** With an AUR account and SSH key:
   `git clone ssh://aur@aur.archlinux.org/stockwise-bin.git`, copy in
   `dist/aur/stockwise-bin/PKGBUILD` and `.SRCINFO`, test with `makepkg -si`
   (and `namcap`), then commit and push. Repeat per release.
5. **Scoop bucket (optional).** The release URL already works. For
   `scoop install stockwise`, create `iamsaquib8/scoop-bucket` with
   `bucket/stockwise.json`; Scoop's `checkver`/`autoupdate` keep it current.

## Checking changes

`.github/workflows/packaging.yml` runs on pull requests that touch these files:
it renders the templates, compares the AUR `.SRCINFO` with `makepkg
--printsrcinfo`, builds and smoke-tests the Docker image, and runs
`nix flake check`. Locally:

```bash
docker build -t stockwise . && docker run --rm stockwise --version
nix flake check && nix run . -- --version
```

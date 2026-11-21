# UltraSearch — Cross-platform

A Rust + egui desktop file-search application for Windows and Linux.

## Current implementation (0.2.0)

- Select one or more folders to index.
- Recursive filesystem scan on a background thread; the UI remains responsive.
- Search indexed file and folder names and paths.
- Search operators: `name:`, `ext:`, `path:`, `type:file`, `type:folder`, `size>10MB`, and `size<500KB`.
- Case-sensitive toggle and optional folder results.
- Basic text preview for common text formats, with a 48 KiB preview limit.
- Open the selected item using the operating system's default application.
- One shared Rust codebase for Windows and Linux.

Search terms are currently combined with AND. The index is in memory and is rebuilt when the user starts indexing; persistent indexes, incremental filesystem watching, advanced Boolean/grouped query syntax, content indexing, thumbnail previews, and installer packages are not implemented yet.

## Run from source

Install the stable Rust toolchain, then:

```sh
cd app
cargo run
```

Linux build prerequisites (Ubuntu/Debian):

```sh
sudo apt-get install build-essential pkg-config libgtk-3-dev libxkbcommon-dev libwayland-dev libx11-dev libxcb1-dev libgl1-mesa-dev
```

## Build release binary

```sh
cd app
cargo build --release
```

The executable is written to `app/target/release/` (or the target-specific subdirectory when cross-compiling).

## Automated builds

GitHub Actions builds a Linux x86_64 tarball and a Windows x86_64 ZIP on pushes and pull requests. Push a version tag such as `v0.2.0` to publish both artifacts as a GitHub Release.

## Important limitations

This is an initial functional foundation, not yet a feature-complete UltraSearch Pro replacement. Indexing very large trees can consume substantial memory, and inaccessible paths are skipped. Do not index sensitive locations unless you intend the application to read their filenames and metadata. The application does not upload indexed data.

## Linux distribution: Flatpak / Flathub

The project includes a Flatpak manifest (`io.github.elrbat.UltraSearch.yml`) and desktop integration metadata. Flatpak is the recommended Linux distribution format because it uses the same application bundle across distributions such as Fedora, CachyOS, and Ubuntu, provided Flatpak is installed and Flathub is enabled.

### Install Flatpak support

- **Fedora:** `sudo dnf install flatpak`
- **CachyOS / Arch-based:** `sudo pacman -S flatpak`
- **Ubuntu:** `sudo apt install flatpak`

Enable the Flathub remote (one-time setup):

```sh
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
```

After UltraSearch is published on Flathub, install and run it with:

```sh
flatpak install flathub io.github.elrbat.UltraSearch
flatpak run io.github.elrbat.UltraSearch
```

The GitHub Actions workflow also builds a standalone `.flatpak` bundle for x86_64 and attaches it to CI artifacts/releases. To prepare the offline Cargo dependency manifest locally, run `./packaging/generate-flatpak-sources.sh` before invoking `flatpak-builder`. This repository currently contains the packaging manifest, but the app is **not yet published on Flathub**; publication requires a separate Flathub submission and review.

### Flatpak filesystem access

The current search engine scans arbitrary user-selected folders, including mounted drives. The manifest therefore grants `--filesystem=host`, which is broader access than ideal for a sandboxed app. This is documented deliberately rather than hiding the permission. A future portal-based file-access design could reduce the permission scope, but would require adapting the indexing implementation and testing it against the relevant desktop portals.

To install a CI-generated standalone bundle before Flathub publication:

```sh
flatpak install --user ./UltraSearch-x86_64.flatpak
flatpak run io.github.elrbat.UltraSearch
```


## License

UltraSearch is distributed under the **GNU General Public License, version 3 only** (`GPL-3.0-only`). See [`LICENSE`](LICENSE) for the complete license text.

This applies to the application source code. The AppStream metadata is separately marked `CC0-1.0` as its metadata license.

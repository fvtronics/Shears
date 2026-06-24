<p align="center">
  <img src="data/icons/com.fvtronics.quire.svg" width="128" alt="Quire Logo"/>
</p>

<h1 align="center">Quire</h1>

<p align="center">
  Simple GNOME utility for working with local PDF files
</p>

<div align="center">

[![Relm4](https://img.shields.io/badge/Relm4-0.11.0-orange)](https://relm4.org)
[![GTK 4](https://img.shields.io/badge/GTK-4-blue?logo=gtk)](https://gtk.org)
[![Platform Linux](https://img.shields.io/badge/Platform-Linux-brightgreen)](#how-to-install)
[![License](https://img.shields.io/badge/License-GPL--3.0--or--later-blue)](COPYING)


</div>

Merge PDFs, organize pages, extract page ranges, split documents,
compress files, add watermarks, and edit metadata, all without relying on online services.

## Screenshots

| Merge PDFs | Split documents | Edit metadata |
|:-----------:|:-----------:|:-----------:|
| ![Merge PDFs](data/resources/screenshots/merge.png?raw=true "Merge multiple PDF files") | ![Split documents](data/resources/screenshots/split.png?raw=true "Split PDF documents") | ![Edit metadata](data/resources/screenshots/metadata.png?raw=true "Edit metadata") |

## How to install

### Flatpak

Quire can be built and installed locally with Flatpak Builder:

```sh
flatpak-builder --user --install --install-deps-from=flathub build-dir com.fvtronics.Quire.json --force-clean
```

### Build from source

Quire uses the [meson build system](http://mesonbuild.com/). Run the following
commands to clone Quire and initialize the build:

```sh
git clone https://codeberg.org/fvtronics/quire.git
cd quire
meson setup build
```

To install the built package on your system, run the following command:

```sh
meson install -C build
```

## License

Licensed under the GPLv3. See the
[COPYING](COPYING) file for the
full license information.

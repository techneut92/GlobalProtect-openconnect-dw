# Build + package the GP Client GUI (gpgui) from the monorepo.
#
#   make build    -> build the Tauri GUI binary (target/release/gpgui)
#   make flatpak  -> build + install the GUI as a Flatpak (the GUI's shipping
#                    format); it talks to the host backend over D-Bus.
#
# The BACKEND (gpservice/gpclient/gpauth) and its native .deb/.rpm/.apk/PKGBUILD
# are built from the top-level Makefile (`<repo>/Makefile`), not here. To build a
# native package that ALSO bundles this GUI, use the top-level `make install
# INCLUDE_GUI=1` (never the upstream proprietary gpgui).
#
# Run inside the gp-build distrobox (the Rust toolchain lives there).
# Flatpak needs `flatpak-builder`.

ROOT := ../..

.PHONY: all build flatpak clean

all: build

build:
	cd $(ROOT) && cargo build --release -p gpgui

flatpak:
	flatpak-builder --force-clean --user --install build-flatpak packaging/flatpak/io.github.techneut92.gpgui.yml

clean:
	rm -rf build-flatpak .flatpak-builder
